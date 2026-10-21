#! /usr/bin/env perl
# Copyright 2005-2016 The OpenSSL Project Authors. All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.


# ====================================================================
# Written by Andy Polyakov <appro@openssl.org> for the OpenSSL
# project.
# ====================================================================

# October 2005.
#
# Montgomery multiplication routine for x86_64. While it gives modest
# 9 
# than twice, >2x, as fast. Most common rsa1024 sign is improved by
# respectful 50 
# dedicated squaring routine can provide further improvement...

# July 2011.
#
# Add dedicated squaring procedure. Performance improvement varies
# from platform to platform, but in average it's ~5%/15%/25%/33%
# for 512-/1024-/2048-/4096-bit RSA *sign* benchmarks respectively.

# August 2011.
#
# Unroll and modulo-schedule inner loops in such manner that they
# are "fallen through" for input lengths of 8, which is critical for
# 1024-bit RSA *sign*. Average performance improvement in comparison
# to *initial* version of this module from 2005 is ~0%/30%/40%/45%
# for 512-/1024-/2048-/4096-bit RSA *sign* benchmarks respectively.

# June 2013.
#
# Optimize reduction in squaring procedure and improve 1024+-bit RSA
# sign performance by 10-16% on Intel Sandy Bridge and later
# (virtually same on non-Intel processors).

# August 2013.
#
# Add MULX/ADOX/ADCX code path.

$flavour = shift;
$output  = shift;
if ($flavour =~ /\./) { $output = $flavour; undef $flavour; }

$win64=0; $win64=1 if ($flavour =~ /[nm]asm|mingw64/ || $output =~ /\.asm$/);

$0 =~ m/(.*[\/\\])[^\/\\]+$/; $dir=$1;
( $xlate="${dir}x86_64-xlate.pl" and -f $xlate ) or
( $xlate="${dir}../../../perlasm/x86_64-xlate.pl" and -f $xlate) or
die "can't locate x86_64-xlate.pl";

open OUT,"| \"$^X\" \"$xlate\" $flavour \"$output\"";
*STDOUT=*OUT;

# In upstream, this is controlled by shelling out to the compiler to check
# versions, but BoringSSL is intended to be used with pre-generated perlasm
# output, so this isn't useful anyway.
$addx = 1;

# void bn_mul_mont_nohw(
$rp="%rdi";	# BN_ULONG *rp,
$ap="%rsi";	# const BN_ULONG *ap,
$bp="%rdx";	# const BN_ULONG *bp,
$np="%rcx";	# const BN_ULONG *np,
$n0="%r8";	# const BN_ULONG *n0,
# TODO(davidben): The code below treats $num as an int, but C passes in a
# size_t.
$num="%r9";	# size_t num);
$lo0="%r10";
$hi0="%r11";
$hi1="%r13";
$i="%r14";
$j="%r15";
$m0="%rbx";
$m1="%rbp";

$code=<<___;
.text

.globl	bn_mul_mont_nohw
.type	bn_mul_mont_nohw,\@function,6
.align	16
bn_mul_mont_nohw:
.cfi_startproc
	_CET_ENDBR
	mov	${num}d,${num}d
	mov	%rsp,%rax
.cfi_def_cfa_register	%rax
	push	%rbx
.cfi_push	%rbx
	push	%rbp
.cfi_push	%rbp
	push	%r12
.cfi_push	%r12
	push	%r13
.cfi_push	%r13
	push	%r14
.cfi_push	%r14
	push	%r15
.cfi_push	%r15

	neg	$num
	mov	%rsp,%r11
	lea	-16(%rsp,$num,8),%r10	# future alloca(8*(num+2))
	neg	$num			# restore $num
	and	\$-1024,%r10		# minimize TLB usage

	# An OS-agnostic version of __chkstk.
	#
	# Some OSes (Windows) insist on stack being "wired" to
	# physical memory in strictly sequential manner, i.e. if stack
	# allocation spans two pages, then reference to farmost one can
	# be punishable by SEGV. But page walking can do good even on
	# other OSes, because it guarantees that villain thread hits
	# the guard page before it can make damage to innocent one...
	sub	%r10,%r11
	and	\$-4096,%r11
	lea	(%r10,%r11),%rsp
	mov	(%rsp),%r11
	cmp	%r10,%rsp
	ja	.Lmul_page_walk
	jmp	.Lmul_page_walk_done

.align	16
.Lmul_page_walk:
	lea	-4096(%rsp),%rsp
	mov	(%rsp),%r11
	cmp	%r10,%rsp
	ja	.Lmul_page_walk
.Lmul_page_walk_done:

	mov	%rax,8(%rsp,$num,8)	# tp[num+1]=%rsp
.cfi_cfa_expression	%rsp+8,$num,8,mul,plus,deref,+8
.Lmul_body:
	mov	$bp,%r12		# reassign $bp
___
		$bp="%r12";
$code.=<<___;
	mov	($n0),$n0		# pull n0[0] value
	mov	($bp),$m0		# m0=bp[0]
	mov	($ap),%rax

	xor	$i,$i			# i=0
	xor	$j,$j			# j=0

	mov	$n0,$m1
	mulq	$m0			# ap[0]*bp[0]
	mov	%rax,$lo0
	mov	($np),%rax

	imulq	$lo0,$m1		# "tp[0]"*n0
	mov	%rdx,$hi0

	mulq	$m1			# np[0]*m1
	add	%rax,$lo0		# discarded
	mov	8($ap),%rax
	adc	\$0,%rdx
	mov	%rdx,$hi1

	lea	1($j),$j		# j++
	jmp	.L1st_enter

.align	16
.L1st:
	add	%rax,$hi1
	mov	($ap,$j,8),%rax
	adc	\$0,%rdx
	add	$hi0,$hi1		# np[j]*m1+ap[j]*bp[0]
	mov	$lo0,$hi0
	adc	\$0,%rdx
	mov	$hi1,-16(%rsp,$j,8)	# tp[j-1]
	mov	%rdx,$hi1

.L1st_enter:
	mulq	$m0			# ap[j]*bp[0]
	add	%rax,$hi0
	mov	($np,$j,8),%rax
	adc	\$0,%rdx
	lea	1($j),$j		# j++
	mov	%rdx,$lo0

	mulq	$m1			# np[j]*m1
	cmp	$num,$j
	jne	.L1st

	add	%rax,$hi1
	mov	($ap),%rax		# ap[0]
	adc	\$0,%rdx
	add	$hi0,$hi1		# np[j]*m1+ap[j]*bp[0]
	adc	\$0,%rdx
	mov	$hi1,-16(%rsp,$j,8)	# tp[j-1]
	mov	%rdx,$hi1
	mov	$lo0,$hi0

	xor	%rdx,%rdx
	add	$hi0,$hi1
	adc	\$0,%rdx
	mov	$hi1,-8(%rsp,$num,8)
	mov	%rdx,(%rsp,$num,8)	# store upmost overflow bit

	lea	1($i),$i		# i++
	jmp	.Louter
.align	16
.Louter:
	mov	($bp,$i,8),$m0		# m0=bp[i]
	xor	$j,$j			# j=0
	mov	$n0,$m1
	mov	(%rsp),$lo0
	mulq	$m0			# ap[0]*bp[i]
	add	%rax,$lo0		# ap[0]*bp[i]+tp[0]
	mov	($np),%rax
	adc	\$0,%rdx

	imulq	$lo0,$m1		# tp[0]*n0
	mov	%rdx,$hi0

	mulq	$m1			# np[0]*m1
	add	%rax,$lo0		# discarded
	mov	8($ap),%rax
	adc	\$0,%rdx
	mov	8(%rsp),$lo0		# tp[1]
	mov	%rdx,$hi1

	lea	1($j),$j		# j++
	jmp	.Linner_enter

.align	16
.Linner:
	add	%rax,$hi1
	mov	($ap,$j,8),%rax
	adc	\$0,%rdx
	add	$lo0,$hi1		# np[j]*m1+ap[j]*bp[i]+tp[j]
	mov	(%rsp,$j,8),$lo0
	adc	\$0,%rdx
	mov	$hi1,-16(%rsp,$j,8)	# tp[j-1]
	mov	%rdx,$hi1

.Linner_enter:
	mulq	$m0			# ap[j]*bp[i]
	add	%rax,$hi0
	mov	($np,$j,8),%rax
	adc	\$0,%rdx
	add	$hi0,$lo0		# ap[j]*bp[i]+tp[j]
	mov	%rdx,$hi0
	adc	\$0,$hi0
	lea	1($j),$j		# j++

	mulq	$m1			# np[j]*m1
	cmp	$num,$j
	jne	.Linner

	add	%rax,$hi1
	mov	($ap),%rax		# ap[0]
	adc	\$0,%rdx
	add	$lo0,$hi1		# np[j]*m1+ap[j]*bp[i]+tp[j]
	mov	(%rsp,$j,8),$lo0
	adc	\$0,%rdx
	mov	$hi1,-16(%rsp,$j,8)	# tp[j-1]
	mov	%rdx,$hi1

	xor	%rdx,%rdx
	add	$hi0,$hi1
	adc	\$0,%rdx
	add	$lo0,$hi1		# pull upmost overflow bit
	adc	\$0,%rdx
	mov	$hi1,-8(%rsp,$num,8)
	mov	%rdx,(%rsp,$num,8)	# store upmost overflow bit

	lea	1($i),$i		# i++
	cmp	$num,$i
	jb	.Louter

	xor	$i,$i			# i=0 and clear CF!
	mov	(%rsp),%rax		# tp[0]
	mov	$num,$j			# j=num

.align	16
.Lsub:	sbb	($np,$i,8),%rax
	mov	%rax,($rp,$i,8)		# rp[i]=tp[i]-np[i]
	mov	8(%rsp,$i,8),%rax	# tp[i+1]
	lea	1($i),$i		# i++
	dec	$j			# doesn't affect CF!
	jnz	.Lsub

	sbb	\$0, 
	mov	\$-1, 
	xor	
	xor	$i,$i
	mov	$num,$j			# j=num

.Lcopy:					# conditional copy
	mov	($rp,$i,8), 
	mov	( 
	and	
	and	
	mov	$num,( 
	or	
	mov	
	lea	1($i),$i
	sub	\$1,$j
	jnz	.Lcopy

	mov	8( 
.cfi_def_cfa	
	mov	\$1, 
	mov	-48( 
.cfi_restore	
	mov	-40( 
.cfi_restore	
	mov	-32( 
.cfi_restore	
	mov	-24( 
.cfi_restore	
	mov	-16( 
.cfi_restore	
	mov	-8( 
.cfi_restore	
	lea	( 
.cfi_def_cfa_register	
.Lmul_epilogue:
	ret
.cfi_endproc
.size	bn_mul_mont_nohw,.-bn_mul_mont_nohw
___
{{{
my @A=("%r10","%r11");
my @N=("%r13","%rdi");
$code.=<<___;
.globl	bn_mul4x_mont
.type	bn_mul4x_mont,\@function,6
.align	16
bn_mul4x_mont:
.cfi_startproc
	_CET_ENDBR
	mov	${num}d,${num}d
	mov	
.cfi_def_cfa_register	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	

	neg	$num
	mov	
	lea	-32( 
	neg	$num			# restore
	and	\$-1024, 

	sub	
	and	\$-4096, 
	lea	( 
	mov	( 
	cmp	
	ja	.Lmul4x_page_walk
	jmp	.Lmul4x_page_walk_done

.Lmul4x_page_walk:
	lea	-4096( 
	mov	( 
	cmp	
	ja	.Lmul4x_page_walk
.Lmul4x_page_walk_done:

	mov	
.cfi_cfa_expression	
.Lmul4x_body:
	mov	$rp,16( 
	mov	
___
		$bp="%r12";
$code.=<<___;
	mov	($n0),$n0		# pull n0[0] value
	mov	($bp),$m0		# m0=bp[0]
	mov	($ap), 

	xor	$i,$i			# i=0
	xor	$j,$j			# j=0

	mov	$n0,$m1
	mulq	$m0			# ap[0]*bp[0]
	mov	
	mov	($np), 

	imulq	$A[0],$m1		# "tp[0]"*n0
	mov	

	mulq	$m1			# np[0]*m1
	add	
	mov	8($ap), 
	adc	\$0, 
	mov	

	mulq	$m0
	add	
	mov	8($np), 
	adc	\$0, 
	mov	

	mulq	$m1
	add	
	mov	16($ap), 
	adc	\$0, 
	add	$A[1],$N[1]
	lea	4($j),$j		# j++
	adc	\$0, 
	mov	$N[1],( 
	mov	
	jmp	.L1st4x
.align	16
.L1st4x:
	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-16($np,$j,8), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-24( 
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8($np,$j,8), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$j,8), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[1],-16( 
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	($np,$j,8), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-8( 
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	8($np,$j,8), 
	adc	\$0, 
	lea	4($j),$j		# j++
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-16($ap,$j,8), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[1],-32( 
	mov	
	cmp	$num,$j
	jb	.L1st4x

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-16($np,$j,8), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-24( 
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8($np,$j,8), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[1],-16( 
	mov	

	xor	$N[1],$N[1]
	add	$A[0],$N[0]
	adc	\$0,$N[1]
	mov	$N[0],-8( 
	mov	$N[1],( 

	lea	1($i),$i		# i++
.align	4
.Louter4x:
	mov	($bp,$i,8),$m0		# m0=bp[i]
	xor	$j,$j			# j=0
	mov	( 
	mov	$n0,$m1
	mulq	$m0			# ap[0]*bp[i]
	add	
	mov	($np), 
	adc	\$0, 

	imulq	$A[0],$m1		# tp[0]*n0
	mov	

	mulq	$m1			# np[0]*m1
	add	
	mov	8($ap), 
	adc	\$0, 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	8($np), 
	adc	\$0, 
	add	8( 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	16($ap), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[i]+tp[j]
	lea	4($j),$j		# j+=2
	adc	\$0, 
	mov	$N[1],( 
	mov	
	jmp	.Linner4x
.align	16
.Linner4x:
	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-16($np,$j,8), 
	adc	\$0, 
	add	-16( 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[0],-24( 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-8($np,$j,8), 
	adc	\$0, 
	add	-8( 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$j,8), 
	adc	\$0, 
	add	$A[1],$N[1]
	adc	\$0, 
	mov	$N[1],-16( 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	($np,$j,8), 
	adc	\$0, 
	add	( 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[0],-8( 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	8($np,$j,8), 
	adc	\$0, 
	add	8( 
	adc	\$0, 
	lea	4($j),$j		# j++
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-16($ap,$j,8), 
	adc	\$0, 
	add	$A[1],$N[1]
	adc	\$0, 
	mov	$N[1],-32( 
	mov	
	cmp	$num,$j
	jb	.Linner4x

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-16($np,$j,8), 
	adc	\$0, 
	add	-16( 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j,8), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[0],-24( 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-8($np,$j,8), 
	adc	\$0, 
	add	-8( 
	adc	\$0, 
	lea	1($i),$i		# i++
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap), 
	adc	\$0, 
	add	$A[1],$N[1]
	adc	\$0, 
	mov	$N[1],-16( 
	mov	

	xor	$N[1],$N[1]
	add	$A[0],$N[0]
	adc	\$0,$N[1]
	add	( 
	adc	\$0,$N[1]
	mov	$N[0],-8( 
	mov	$N[1],( 

	cmp	$num,$i
	jb	.Louter4x
___
{
my @ri=("%rax","%rdx",$m0,$m1);
$code.=<<___;
	mov	16( 
	lea	-4($num),$j
	mov	0( 
	mov	8( 
	shr	\$2,$j			# j=num/4-1
	lea	( 
	xor	$i,$i			# i=0 and clear CF!

	sub	0($np),@ri[0]
	mov	16($ap),@ri[2]		# tp[2]
	mov	24($ap),@ri[3]		# tp[3]
	sbb	8($np),@ri[1]

.Lsub4x:
	mov	@ri[0],0($rp,$i,8)	# rp[i]=tp[i]-np[i]
	mov	@ri[1],8($rp,$i,8)	# rp[i]=tp[i]-np[i]
	sbb	16($np,$i,8),@ri[2]
	mov	32($ap,$i,8),@ri[0]	# tp[i+1]
	mov	40($ap,$i,8),@ri[1]
	sbb	24($np,$i,8),@ri[3]
	mov	@ri[2],16($rp,$i,8)	# rp[i]=tp[i]-np[i]
	mov	@ri[3],24($rp,$i,8)	# rp[i]=tp[i]-np[i]
	sbb	32($np,$i,8),@ri[0]
	mov	48($ap,$i,8),@ri[2]
	mov	56($ap,$i,8),@ri[3]
	sbb	40($np,$i,8),@ri[1]
	lea	4($i),$i		# i++
	dec	$j			# doesn't affect CF!
	jnz	.Lsub4x

	mov	@ri[0],0($rp,$i,8)	# rp[i]=tp[i]-np[i]
	mov	32($ap,$i,8),@ri[0]	# load overflow bit
	sbb	16($np,$i,8),@ri[2]
	mov	@ri[1],8($rp,$i,8)	# rp[i]=tp[i]-np[i]
	sbb	24($np,$i,8),@ri[3]
	mov	@ri[2],16($rp,$i,8)	# rp[i]=tp[i]-np[i]

	sbb	\$0,@ri[0]		# handle upmost overflow bit
	mov	@ri[3],24($rp,$i,8)	# rp[i]=tp[i]-np[i]
	pxor	%xmm0,%xmm0
	movq	@ri[0],%xmm4
	pcmpeqd	%xmm5,%xmm5
	pshufd	\$0,%xmm4,%xmm4
	mov	$num,$j
	pxor	%xmm4,%xmm5
	shr	\$2,$j			# j=num/4
	xor	%eax,%eax		# i=0

	jmp	.Lcopy4x
.align	16
.Lcopy4x:				# conditional copy
	movdqa	(%rsp,%rax),%xmm1
	movdqu	($rp,%rax),%xmm2
	pand	%xmm4,%xmm1
	pand	%xmm5,%xmm2
	movdqa	16(%rsp,%rax),%xmm3
	movdqa	%xmm0,(%rsp,%rax)
	por	%xmm2,%xmm1
	movdqu	16($rp,%rax),%xmm2
	movdqu	%xmm1,($rp,%rax)
	pand	%xmm4,%xmm3
	pand	%xmm5,%xmm2
	movdqa	%xmm0,16(%rsp,%rax)
	por	%xmm2,%xmm3
	movdqu	%xmm3,16($rp,%rax)
	lea	32(%rax),%rax
	dec	$j
	jnz	.Lcopy4x
___
}
$code.=<<___;
	mov	8(%rsp,$num,8),%rsi	# restore %rsp
.cfi_def_cfa	%rsi, 8
	mov	\$1,%rax
	mov	-48(%rsi),%r15
.cfi_restore	%r15
	mov	-40(%rsi),%r14
.cfi_restore	%r14
	mov	-32(%rsi),%r13
.cfi_restore	%r13
	mov	-24(%rsi),%r12
.cfi_restore	%r12
	mov	-16(%rsi),%rbp
.cfi_restore	%rbp
	mov	-8(%rsi),%rbx
.cfi_restore	%rbx
	lea	(%rsi),%rsp
.cfi_def_cfa_register	%rsp
.Lmul4x_epilogue:
	ret
.cfi_endproc
.size	bn_mul4x_mont,.-bn_mul4x_mont
___
}}}
{{{
######################################################################
# void bn_sqr8x_mont(
my $rptr="%rdi";	# const BN_ULONG *rptr,
my $aptr="%rsi";	# const BN_ULONG *aptr,
my $mulx_adx_capable="%rdx"; # Different than upstream!
my $nptr="%rcx";	# const BN_ULONG *nptr,
my $n0  ="%r8";		# const BN_ULONG *n0);
my $num ="%r9";		# int num, has to be divisible by 8

my ($i,$j,$tptr)=("%rbp","%rcx",$rptr);
my @A0=("%r10","%r11");
my @A1=("%r12","%r13");
my ($a0,$a1,$ai)=("%r14","%r15","%rbx");

$code.=<<___	if ($addx);
.extern	bn_sqrx8x_internal		# see x86_64-mont5 module
___
$code.=<<___;
.extern	bn_sqr8x_internal		# see x86_64-mont5 module

.globl	bn_sqr8x_mont
.type	bn_sqr8x_mont,\@function,6
.align	32
bn_sqr8x_mont:
.cfi_startproc
	_CET_ENDBR
	mov	${num}d,${num}d
	mov	%rsp,%rax
.cfi_def_cfa_register	%rax
	push	%rbx
.cfi_push	%rbx
	push	%rbp
.cfi_push	%rbp
	push	%r12
.cfi_push	%r12
	push	%r13
.cfi_push	%r13
	push	%r14
.cfi_push	%r14
	push	%r15
.cfi_push	%r15
.Lsqr8x_prologue:

	mov	${num}d,%r10d
	shl	\$3,${num}d		# convert $num to bytes
	shl	\$3+2,%r10		# 4*$num
	neg	$num

	##############################################################
	# ensure that stack frame doesn't alias with $aptr modulo
	# 4096. this is done to allow memory disambiguation logic
	# do its job.
	#
	lea	-64( 
	mov	
	mov	($n0),$n0		# *n0
	sub	$aptr, 
	and	\$4095, 
	cmp	
	jb	.Lsqr8x_sp_alt
	sub	
	lea	-64( 
	jmp	.Lsqr8x_sp_done

.align	32
.Lsqr8x_sp_alt:
	lea	4096-64(,$num,2), 
	lea	-64( 
	sub	
	mov	\$0, 
	cmovc	
	sub	
.Lsqr8x_sp_done:
	and	\$-64, 
	mov	
	sub	
	and	\$-4096, 
	lea	( 
	mov	( 
	cmp	
	ja	.Lsqr8x_page_walk
	jmp	.Lsqr8x_page_walk_done

.align	16
.Lsqr8x_page_walk:
	lea	-4096( 
	mov	( 
	cmp	
	ja	.Lsqr8x_page_walk
.Lsqr8x_page_walk_done:

	mov	$num, 
	neg	$num

	mov	$n0,  32( 
	mov	
.cfi_cfa_expression	
.Lsqr8x_body:

	movq	$nptr, 
	pxor	
	movq	$rptr, 
	movq	
___
$code.=<<___ if ($addx);
	test	$mulx_adx_capable,$mulx_adx_capable
	jz	.Lsqr8x_nox

	call	bn_sqrx8x_internal	# see x86_64-mont5 module
					# 
					# 
					# 
					# 
	lea	( 
	mov	
	mov	
	movq	
	sar	\$3+2, 
	jmp	.Lsqr8x_sub

.align	32
.Lsqr8x_nox:
___
$code.=<<___;
	call	bn_sqr8x_internal	# see x86_64-mont5 module
					# 
					# 
					# 
					# 
	lea	( 
	mov	$num, 
	mov	$num, 
	movq	
	sar	\$3+2, 
	jmp	.Lsqr8x_sub

.align	32
.Lsqr8x_sub:
	mov	8*0( 
	mov	8*1( 
	mov	8*2( 
	mov	8*3( 
	lea	8*4( 
	sbb	8*0( 
	sbb	8*1( 
	sbb	8*2( 
	sbb	8*3( 
	lea	8*4( 
	mov	
	mov	
	mov	
	mov	
	lea	8*4($rptr),$rptr
	inc	
	jnz	.Lsqr8x_sub

	sbb	\$0, 
	lea	( 
	lea	($rptr,$num),$rptr	# rewind

	movq	
	pxor	
	pshufd	\$0, 
	mov	40( 
.cfi_def_cfa	
	jmp	.Lsqr8x_cond_copy

.align	32
.Lsqr8x_cond_copy:
	movdqa	16*0( 
	movdqa	16*1( 
	lea	16*2( 
	movdqu	16*0($rptr), 
	movdqu	16*1($rptr), 
	lea	16*2($rptr),$rptr
	movdqa	
	movdqa	
	movdqa	
	movdqa	
	pcmpeqd	
	pand	
	pand	
	pand	
	pand	
	pxor	
	por	
	por	
	movdqu	
	movdqu	
	add	\$32,$num
	jnz	.Lsqr8x_cond_copy

	mov	\$1, 
	mov	-48( 
.cfi_restore	
	mov	-40( 
.cfi_restore	
	mov	-32( 
.cfi_restore	
	mov	-24( 
.cfi_restore	
	mov	-16( 
.cfi_restore	
	mov	-8( 
.cfi_restore	
	lea	( 
.cfi_def_cfa_register	
.Lsqr8x_epilogue:
	ret
.cfi_endproc
.size	bn_sqr8x_mont,.-bn_sqr8x_mont
___
}}}

if ($addx) {{{
my $bp="%rdx";	# original value

$code.=<<___;
.globl	bn_mulx4x_mont
.type	bn_mulx4x_mont,\@function,6
.align	32
bn_mulx4x_mont:
.cfi_startproc
	_CET_ENDBR
	mov	
.cfi_def_cfa_register	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
	push	
.cfi_push	
.Lmulx4x_prologue:

	shl	\$3,${num}d		# convert $num to bytes
	xor	
	sub	$num, 
	mov	($n0),$n0		# *n0
	lea	-72( 
	and	\$-128, 
	mov	
	sub	
	and	\$-4096, 
	lea	( 
	mov	( 
	cmp	
	ja	.Lmulx4x_page_walk
	jmp	.Lmulx4x_page_walk_done

.align	16
.Lmulx4x_page_walk:
	lea	-4096( 
	mov	( 
	cmp	
	ja	.Lmulx4x_page_walk
.Lmulx4x_page_walk_done:

	lea	($bp,$num), 
	##############################################################
	# Stack layout
	# +0	num
	# +8	off-loaded &b[i]
	# +16	end of b[num]
	# +24	saved n0
	# +32	saved rp
	# +40	saved 
	# +48	inner counter
	# +56
	# +64	tmp[num+1]
	#
	mov	$num,0( 
	shr	\$5,$num
	mov	
	sub	\$1,$num
	mov	$n0, 24( 
	mov	$rp, 32( 
	mov	
.cfi_cfa_expression	
	mov	$num,48( 
	jmp	.Lmulx4x_body

.align	32
.Lmulx4x_body:
___
my ($aptr, $bptr, $nptr, $tptr, $mi,  $bi,  $zero, $num)=
   ("%rsi","%rdi","%rcx","%rbx","%r8","%r9","%rbp","%rax");
my $rptr=$bptr;
$code.=<<___;
	lea	8($bp),$bptr
	mov	($bp), 
	lea	64+32( 
	mov	

	mulx	0*8($aptr),$mi, 
	mulx	1*8($aptr), 
	add	
	mov	$bptr,8( 
	mulx	2*8($aptr), 
	adc	
	adc	\$0, 

	mov	$mi,$bptr		# borrow $bptr
	imulq	24( 
	xor	$zero,$zero		# cf=0, of=0

	mulx	3*8($aptr), 
	 mov	$mi, 
	lea	4*8($aptr),$aptr
	adcx	
	adcx	$zero, 

	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	.byte	0xc4,0x62,0xfb,0xf6,0xa1,0x10,0x00,0x00,0x00	# mulx	2*8($nptr), 
	mov	48( 
	mov	
	adcx	
	adox	
	mulx	3*8($nptr), 
	 mov	$bi, 
	mov	
	adcx	
	adox	$zero, 
	lea	4*8($nptr),$nptr
	mov	

	jmp	.Lmulx4x_1st

.align	32
.Lmulx4x_1st:
	adcx	$zero, 
	mulx	0*8($aptr), 
	adcx	
	mulx	1*8($aptr), 
	adcx	
	mulx	2*8($aptr), 
	adcx	
	mulx	3*8($aptr), 
	 .byte	0x67,0x67
	 mov	$mi, 
	adcx	
	adcx	$zero, 
	lea	4*8($aptr),$aptr
	lea	4*8($tptr),$tptr

	adox	
	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	mulx	2*8($nptr), 
	mov	
	adcx	
	mov	
	adox	
	mulx	3*8($nptr), 
	 mov	$bi, 
	mov	
	adcx	
	adox	$zero, 
	lea	4*8($nptr),$nptr
	mov	

	dec	$bptr			# of=0, pass cf
	jnz	.Lmulx4x_1st

	mov	0( 
	mov	8( 
	adc	$zero, 
	add	
	sbb	
	mov	
	jmp	.Lmulx4x_outer

.align	32
.Lmulx4x_outer:
	mov	($bptr), 
	lea	8($bptr),$bptr		# b++
	sub	$num,$aptr		# rewind $aptr
	mov	
	lea	64+4*8( 
	sub	$num,$nptr		# rewind $nptr

	mulx	0*8($aptr),$mi, 
	xor	
	mov	
	mulx	1*8($aptr), 
	adox	-4*8($tptr),$mi
	adcx	
	mulx	2*8($aptr), 
	adox	-3*8($tptr), 
	adcx	
	adox	-2*8($tptr), 
	adcx	$zero, 
	adox	$zero, 

	mov	$bptr,8( 
	mov	$mi, 
	imulq	24( 
	xor	

	mulx	3*8($aptr), 
	 mov	$mi, 
	adcx	
	adox	-1*8($tptr), 
	adcx	$zero, 
	lea	4*8($aptr),$aptr
	adox	$zero, 

	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	mulx	2*8($nptr), 
	mov	
	adcx	
	adox	
	mulx	3*8($nptr), 
	 mov	$bi, 
	mov	
	lea	4*8($nptr),$nptr
	adcx	
	adox	$zero, 
	mov	48( 
	mov	

	jmp	.Lmulx4x_inner

.align	32
.Lmulx4x_inner:
	mulx	0*8($aptr), 
	adcx	$zero, 
	adox	
	mulx	1*8($aptr), 
	adcx	0*8($tptr), 
	adox	
	mulx	2*8($aptr), 
	adcx	1*8($tptr), 
	adox	
	mulx	3*8($aptr), 
	 mov	$mi, 
	adcx	2*8($tptr), 
	adox	
	adcx	3*8($tptr), 
	adox	$zero, 
	lea	4*8($aptr),$aptr
	lea	4*8($tptr),$tptr
	adcx	$zero, 

	adox	
	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	mulx	2*8($nptr), 
	mov	
	adcx	
	adox	
	mulx	3*8($nptr), 
	 mov	$bi, 
	mov	
	mov	
	adcx	
	adox	$zero, 
	lea	4*8($nptr),$nptr
	mov	

	dec	$bptr			# of=0, pass cf
	jnz	.Lmulx4x_inner

	mov	0( 
	mov	8( 
	adc	$zero, 
	sub	0*8($tptr),$zero	# pull top-most carry
	adc	
	sbb	
	mov	

	cmp	16( 
	jne	.Lmulx4x_outer

	lea	64( 
	sub	$num,$nptr		# rewind $nptr
	neg	
	mov	$num, 
	shr	\$3+2,$num		# 
	mov	32( 
	jmp	.Lmulx4x_sub

.align	32
.Lmulx4x_sub:
	mov	8*0($tptr), 
	mov	8*1($tptr), 
	mov	8*2($tptr), 
	mov	8*3($tptr), 
	lea	8*4($tptr),$tptr
	sbb	8*0($nptr), 
	sbb	8*1($nptr), 
	sbb	8*2($nptr), 
	sbb	8*3($nptr), 
	lea	8*4($nptr),$nptr
	mov	
	mov	
	mov	
	mov	
	lea	8*4($rptr),$rptr
	dec	$num			# preserves 
	jnz	.Lmulx4x_sub

	sbb	\$0, 
	lea	64( 
	sub	

	movq	
	pxor	
	pshufd	\$0, 
	mov	40( 
.cfi_def_cfa	
	jmp	.Lmulx4x_cond_copy

.align	32
.Lmulx4x_cond_copy:
	movdqa	16*0($tptr), 
	movdqa	16*1($tptr), 
	lea	16*2($tptr),$tptr
	movdqu	16*0($rptr), 
	movdqu	16*1($rptr), 
	lea	16*2($rptr),$rptr
	movdqa	
	movdqa	
	pcmpeqd	
	pand	
	pand	
	pand	
	pand	
	pxor	
	por	
	por	
	movdqu	
	movdqu	
	sub	\$32, 
	jnz	.Lmulx4x_cond_copy

	mov	

	mov	\$1, 
	mov	-48( 
.cfi_restore	
	mov	-40( 
.cfi_restore	
	mov	-32( 
.cfi_restore	
	mov	-24( 
.cfi_restore	
	mov	-16( 
.cfi_restore	
	mov	-8( 
.cfi_restore	
	lea	( 
.cfi_def_cfa_register	
.Lmulx4x_epilogue:
	ret
.cfi_endproc
.size	bn_mulx4x_mont,.-bn_mulx4x_mont
___
}}}
$code.=<<___;
.asciz	"Montgomery Multiplication for x86_64, CRYPTOGAMS by <appro\@openssl.org>"
.align	16
___

# EXCEPTION_DISPOSITION handler (EXCEPTION_RECORD *rec,ULONG64 frame,
#		CONTEXT *context,DISPATCHER_CONTEXT *disp)
if ($win64) {
$rec=" 
$frame="%rdx";
$context="%r8";
$disp="%r9";

$code.=<<___;
.extern	__imp_RtlVirtualUnwind
.type	mul_handler,\@abi-omnipotent
.align	16
mul_handler:
	push	
	push	
	push	
	push	
	push	
	push	
	push	
	push	
	pushfq
	sub	\$64, 

	mov	120($context), 
	mov	248($context), 

	mov	8($disp), 
	mov	56($disp), 

	mov	0( 
	lea	( 
	cmp	
	jb	.Lcommon_seh_tail

	mov	152($context), 

	mov	4( 
	lea	( 
	cmp	
	jae	.Lcommon_seh_tail

	mov	192($context), 
	mov	8( 

	jmp	.Lcommon_pop_regs
.size	mul_handler,.-mul_handler

.type	sqr_handler,\@abi-omnipotent
.align	16
sqr_handler:
	push	
	push	
	push	
	push	
	push	
	push	
	push	
	push	
	pushfq
	sub	\$64, 

	mov	120($context), 
	mov	248($context), 

	mov	8($disp), 
	mov	56($disp), 

	mov	0( 
	lea	( 
	cmp	
	jb	.Lcommon_seh_tail

	mov	4( 
	lea	( 
	cmp	
	jb	.Lcommon_pop_regs

	mov	152($context), 

	mov	8( 
	lea	( 
	cmp	
	jae	.Lcommon_seh_tail

	mov	40( 

.Lcommon_pop_regs:
	mov	-8( 
	mov	-16( 
	mov	-24( 
	mov	-32( 
	mov	-40( 
	mov	-48( 
	mov	
	mov	
	mov	
	mov	
	mov	
	mov	

.Lcommon_seh_tail:
	mov	8( 
	mov	16( 
	mov	
	mov	
	mov	

	mov	40($disp), 
	mov	$context, 
	mov	\$154, 
	.long	0xa548f3fc		# cld; rep movsq

	mov	$disp, 
	xor	
	mov	8( 
	mov	0( 
	mov	16( 
	mov	40( 
	lea	56( 
	lea	24( 
	mov	
	mov	
	mov	
	mov	
	call	*__imp_RtlVirtualUnwind( 

	mov	\$1, 
	add	\$64, 
	popfq
	pop	
	pop	
	pop	
	pop	
	pop	
	pop	
	pop	
	pop	
	ret
.size	sqr_handler,.-sqr_handler

.section	.pdata
.align	4
	.rva	.LSEH_begin_bn_mul_mont_nohw
	.rva	.LSEH_end_bn_mul_mont_nohw
	.rva	.LSEH_info_bn_mul_mont_nohw

	.rva	.LSEH_begin_bn_mul4x_mont
	.rva	.LSEH_end_bn_mul4x_mont
	.rva	.LSEH_info_bn_mul4x_mont

	.rva	.LSEH_begin_bn_sqr8x_mont
	.rva	.LSEH_end_bn_sqr8x_mont
	.rva	.LSEH_info_bn_sqr8x_mont
___
$code.=<<___ if ($addx);
	.rva	.LSEH_begin_bn_mulx4x_mont
	.rva	.LSEH_end_bn_mulx4x_mont
	.rva	.LSEH_info_bn_mulx4x_mont
___
$code.=<<___;
.section	.xdata
.align	8
.LSEH_info_bn_mul_mont_nohw:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lmul_body,.Lmul_epilogue	# HandlerData[]
.LSEH_info_bn_mul4x_mont:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lmul4x_body,.Lmul4x_epilogue	# HandlerData[]
.LSEH_info_bn_sqr8x_mont:
	.byte	9,0,0,0
	.rva	sqr_handler
	.rva	.Lsqr8x_prologue,.Lsqr8x_body,.Lsqr8x_epilogue		# HandlerData[]
.align	8
___
$code.=<<___ if ($addx);
.LSEH_info_bn_mulx4x_mont:
	.byte	9,0,0,0
	.rva	sqr_handler
	.rva	.Lmulx4x_prologue,.Lmulx4x_body,.Lmulx4x_epilogue	# HandlerData[]
.align	8
___
}

print $code;
close STDOUT or die "error closing STDOUT: $!";
