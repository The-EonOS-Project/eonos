#! /usr/bin/env perl
# Copyright 2011-2016 The OpenSSL Project Authors. All Rights Reserved.
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

# August 2011.
#
# Companion to x86_64-mont.pl that optimizes cache-timing attack
# countermeasures. The subroutines are produced by replacing bp[i]
# references in their x86_64-mont.pl counterparts with cache-neutral
# references to powers table computed in BN_mod_exp_mont_consttime.
# In addition subroutine that scatters elements of the powers table
# is implemented, so that scatter-/gathering can be tuned without
# bn_exp.c modifications.

# August 2013.
#
# Add MULX/AD*X code paths and additional interfaces to optimize for
# branch prediction unit. For input lengths that are multiples of 8
# the np argument is not just modulus value, but one interleaved
# with 0. This is to optimize post-condition...

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

# int bn_mul_mont_gather5_nohw(
$rp="%rdi";	# BN_ULONG *rp,
$ap="%rsi";	# const BN_ULONG *ap,
$bp="%rdx";	# const BN_ULONG *bp,
$np="%rcx";	# const BN_ULONG *np,
$n0="%r8";	# const BN_ULONG *n0,
$num="%r9";	# int num,
		# int idx);	# 0 to 2^5-1, "index" in $bp holding
				# pre-computed powers of a', interlaced
				# in such manner that b[0] is $bp[idx],
				# b[1] is [2^5+idx], etc.
$lo0="%r10";
$hi0="%r11";
$hi1="%r13";
$i="%r14";
$j="%r15";
$m0="%rbx";
$m1="%rbp";

$code=<<___;
.text

___
{{{
my @A=("%r10","%r11");
my @N=("%r13","%rdi");
$code.=<<___;
.globl	bn_mul4x_mont_gather5
.type	bn_mul4x_mont_gather5,\@function,6
.align	32
bn_mul4x_mont_gather5:
.cfi_startproc
	_CET_ENDBR
	.byte	0x67
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
.Lmul4x_prologue:

	.byte	0x67
	# num is declared as an int, a 32-bit parameter, so the upper half is
	# undefined. It is important that this write to ${num}, which zeros the
	# upper half, predates the first access.
	shl	\$3,${num}d		# convert $num to bytes
	lea	($num,$num,2), 
	neg	$num			# -$num

	##############################################################
	# Ensure that stack frame doesn't alias with $rptr+3*$num
	# modulo 4096, which covers ret[num], am[num] and n[num]
	# (see bn_exp.c). This is done to allow memory disambiguation
	# logic do its magic. [Extra [num] is allocated in order
	# to align with bn_power5's frame, which is cleansed after
	# completing exponentiation. Extra 256 bytes is for power mask
	# calculated from 7th argument, the index.]
	#
	lea	-320( 
	mov	
	sub	$rp, 
	and	\$4095, 
	cmp	
	jb	.Lmul4xsp_alt
	sub	
	lea	-320( 
	jmp	.Lmul4xsp_done

.align	32
.Lmul4xsp_alt:
	lea	4096-320(,$num,2), 
	lea	-320( 
	sub	
	mov	\$0, 
	cmovc	
	sub	
.Lmul4xsp_done:
	and	\$-64, 
	mov	
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

	neg	$num

	mov	
.cfi_cfa_expression	
.Lmul4x_body:

	call	mul4x_internal

	mov	40( 
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
.Lmul4x_epilogue:
	ret
.cfi_endproc
.size	bn_mul4x_mont_gather5,.-bn_mul4x_mont_gather5

.type	mul4x_internal,\@abi-omnipotent
.align	32
mul4x_internal:
.cfi_startproc
	shl	\$5,$num		# $num was in bytes
	movd	`($win64?56:8)`( 
	lea	.Linc( 
	lea	128( 
	shr	\$5,$num		# restore $num
___
		$bp="%r12";
		$STRIDE=2**5*8;		# 5 is "window size"
		$tp=$i;
$code.=<<___;
	movdqa	0( 
	movdqa	16( 
	lea	88-112( 
	lea	128( 

	pshufd	\$0, 
	movdqa	
	.byte	0x67,0x67
	movdqa	
___
########################################################################
# Calculate masks by comparing 0..31 to $idx and save result to stack.
#
# We compute sixteen 16-byte masks and store them on the stack. Mask i is stored
# in `16*i - 128`( 
# idx == 2*i + 1 in its lower and upper halves, respectively. Mask calculations
# are scheduled in groups of four.
$code.=<<___;
	paddd	
	pcmpeqd	
	.byte	0x67
	movdqa	
___
for($i=0;$i<$STRIDE/16-4;$i+=4) {
$code.=<<___;
	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	
___
}
$code.=<<___;				# last iteration can be optimized
	paddd	
	pcmpeqd	
	movdqa	

	paddd	
	.byte	0x67
	pcmpeqd	
	movdqa	

	pcmpeqd	
	movdqa	
	pand	`16*($i+0)-128`($bp), 

	pand	`16*($i+1)-128`($bp), 
	pand	`16*($i+2)-128`($bp), 
	movdqa	
	pand	`16*($i+3)-128`($bp), 
	por	
	por	
___
for($i=0;$i<$STRIDE/16-4;$i+=4) {
$code.=<<___;
	movdqa	`16*($i+0)-128`($bp), 
	movdqa	`16*($i+1)-128`($bp), 
	movdqa	`16*($i+2)-128`($bp), 
	pand	`16*($i+0)+112`( 
	movdqa	`16*($i+3)-128`($bp), 
	pand	`16*($i+1)+112`( 
	por	
	pand	`16*($i+2)+112`( 
	por	
	pand	`16*($i+3)+112`( 
	por	
	por	
___
}
$code.=<<___;
	por	
	# Combine the upper and lower halves of 
	pshufd	\$0x4e, 
	por	
	lea	$STRIDE($bp),$bp
	movq	

	mov	
	mov	$rp, 56+8( 

	mov	($n0),$n0		# pull n0[0] value
	mov	($ap), 
	lea	($ap,$num),$ap		# end of a[num]
	neg	$num

	mov	$n0,$m1
	mulq	$m0			# ap[0]*bp[0]
	mov	
	mov	($np), 

	imulq	$A[0],$m1		# "tp[0]"*n0
	lea	64+8( 
	mov	

	mulq	$m1			# np[0]*m1
	add	
	mov	8($ap,$num), 
	adc	\$0, 
	mov	

	mulq	$m0
	add	
	mov	8*1($np), 
	adc	\$0, 
	mov	

	mulq	$m1
	add	
	mov	16($ap,$num), 
	adc	\$0, 
	add	$A[1],$N[1]
	lea	4*8($num),$j		# j=4
	lea	8*4($np),$np
	adc	\$0, 
	mov	$N[1],($tp)
	mov	
	jmp	.L1st4x

.align	32
.L1st4x:
	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8*2($np), 
	lea	32($tp),$tp
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-24($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8*1($np), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$j), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[1],-16($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	8*0($np), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	8($ap,$j), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-8($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	8*1($np), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	16($ap,$j), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	lea	8*4($np),$np
	adc	\$0, 
	mov	$N[1],($tp)		# tp[j-1]
	mov	

	add	\$32,$j			# j+=4
	jnz	.L1st4x

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8*2($np), 
	lea	32($tp),$tp
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap), 
	adc	\$0, 
	add	$A[0],$N[0]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[0],-24($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[0]
	add	
	mov	-8*1($np), 
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$num), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[0]
	adc	\$0, 
	mov	$N[1],-16($tp)		# tp[j-1]
	mov	

	lea	($np,$num),$np		# rewind $np

	xor	$N[1],$N[1]
	add	$A[0],$N[0]
	adc	\$0,$N[1]
	mov	$N[0],-8($tp)

	jmp	.Louter4x

.align	32
.Louter4x:
	lea	16+128($tp), 
	pxor	
	pxor	
___
for($i=0;$i<$STRIDE/16;$i+=4) {
$code.=<<___;
	movdqa	`16*($i+0)-128`($bp), 
	movdqa	`16*($i+1)-128`($bp), 
	movdqa	`16*($i+2)-128`($bp), 
	movdqa	`16*($i+3)-128`($bp), 
	pand	`16*($i+0)-128`( 
	pand	`16*($i+1)-128`( 
	por	
	pand	`16*($i+2)-128`( 
	por	
	pand	`16*($i+3)-128`( 
	por	
	por	
___
}
$code.=<<___;
	por	
	# Combine the upper and lower halves of 
	pshufd	\$0x4e, 
	por	
	lea	$STRIDE($bp),$bp
	movq	

	mov	($tp,$num),$A[0]
	mov	$n0,$m1
	mulq	$m0			# ap[0]*bp[i]
	add	
	mov	($np), 
	adc	\$0, 

	imulq	$A[0],$m1		# tp[0]*n0
	mov	
	mov	$N[1],($tp)		# store upmost overflow bit

	lea	($tp,$num),$tp		# rewind $tp

	mulq	$m1			# np[0]*m1
	add	
	mov	8($ap,$num), 
	adc	\$0, 
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	8*1($np), 
	adc	\$0, 
	add	8($tp),$A[1]		# +tp[1]
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	16($ap,$num), 
	adc	\$0, 
	add	$A[1],$N[1]		# np[j]*m1+ap[j]*bp[i]+tp[j]
	lea	4*8($num),$j		# j=4
	lea	8*4($np),$np
	adc	\$0, 
	mov	
	jmp	.Linner4x

.align	32
.Linner4x:
	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-8*2($np), 
	adc	\$0, 
	add	16($tp),$A[0]		# ap[j]*bp[i]+tp[j]
	lea	32($tp),$tp
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap,$j), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[1],-32($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-8*1($np), 
	adc	\$0, 
	add	-8($tp),$A[1]
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$j), 
	adc	\$0, 
	add	$A[1],$N[1]
	adc	\$0, 
	mov	$N[0],-24($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	8*0($np), 
	adc	\$0, 
	add	($tp),$A[0]		# ap[j]*bp[i]+tp[j]
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	8($ap,$j), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[1],-16($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	8*1($np), 
	adc	\$0, 
	add	8($tp),$A[1]
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	16($ap,$j), 
	adc	\$0, 
	add	$A[1],$N[1]
	lea	8*4($np),$np
	adc	\$0, 
	mov	$N[0],-8($tp)		# tp[j-1]
	mov	

	add	\$32,$j			# j+=4
	jnz	.Linner4x

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	-8*2($np), 
	adc	\$0, 
	add	16($tp),$A[0]		# ap[j]*bp[i]+tp[j]
	lea	32($tp),$tp
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	-8($ap), 
	adc	\$0, 
	add	$A[0],$N[0]
	adc	\$0, 
	mov	$N[1],-32($tp)		# tp[j-1]
	mov	

	mulq	$m0			# ap[j]*bp[i]
	add	
	mov	$m1, 
	mov	-8*1($np),$m1
	adc	\$0, 
	add	-8($tp),$A[1]
	adc	\$0, 
	mov	

	mulq	$m1			# np[j]*m1
	add	
	mov	($ap,$num), 
	adc	\$0, 
	add	$A[1],$N[1]
	adc	\$0, 
	mov	$N[0],-24($tp)		# tp[j-1]
	mov	

	mov	$N[1],-16($tp)		# tp[j-1]
	lea	($np,$num),$np		# rewind $np

	xor	$N[1],$N[1]
	add	$A[0],$N[0]
	adc	\$0,$N[1]
	add	($tp),$N[0]		# pull upmost overflow bit
	adc	\$0,$N[1]		# upmost overflow bit
	mov	$N[0],-8($tp)

	cmp	16+8( 
	jb	.Louter4x
___
if (1) {
$code.=<<___;
	xor	
	sub	$N[0],$m1		# compare top-most words
	adc	$j,$j			# $j is zero
	or	$j,$N[1]
	sub	$N[1], 
	lea	($tp,$num), 
	mov	($np), 
	lea	($np), 
	mov	
	sar	\$3+2, 
	mov	56+8( 
	dec	
	xor	
	mov	8*1( 
	mov	8*2( 
	mov	8*3( 
	jmp	.Lsqr4x_sub_entry
___
}
$code.=<<___;
.cfi_endproc
.size	mul4x_internal,.-mul4x_internal
___
}}}

{{{
######################################################################
# void bn_power5_nohw(
my $rptr="%rdi";	# BN_ULONG *rptr,
my $aptr="%rsi";	# const BN_ULONG *aptr,
my $bptr="%rdx";	# const void *table,
my $nptr="%rcx";	# const BN_ULONG *nptr,
my $n0  ="%r8";		# const BN_ULONG *n0);
my $num ="%r9";		# int num, has to be divisible by 8
			# int pwr

my ($i,$j,$tptr)=("%rbp","%rcx",$rptr);
my @A0=("%r10","%r11");
my @A1=("%r12","%r13");
my ($a0,$a1,$ai)=("%r14","%r15","%rbx");

$code.=<<___;
.globl	bn_power5_nohw
.type	bn_power5_nohw,\@function,6
.align	32
bn_power5_nohw:
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
.Lpower5_prologue:

	# num is declared as an int, a 32-bit parameter, so the upper half is
	# undefined. It is important that this write to ${num}, which zeros the
	# upper half, come before the first access.
	shl	\$3,${num}d		# convert $num to bytes
	lea	($num,$num,2), 
	neg	$num
	mov	($n0),$n0		# *n0

	##############################################################
	# Ensure that stack frame doesn't alias with $rptr+3*$num
	# modulo 4096, which covers ret[num], am[num] and n[num]
	# (see bn_exp.c). This is done to allow memory disambiguation
	# logic do its magic. [Extra 256 bytes is for power mask
	# calculated from 7th argument, the index.]
	#
	lea	-320(%rsp,$num,2),%r11
	mov	%rsp,%rbp
	sub	$rptr,%r11
	and	\$4095,%r11
	cmp	%r11,%r10
	jb	.Lpwr_sp_alt
	sub	%r11,%rbp		# align with $aptr
	lea	-320(%rbp,$num,2),%rbp	# future alloca(frame+2*num*8+256)
	jmp	.Lpwr_sp_done

.align	32
.Lpwr_sp_alt:
	lea	4096-320(,$num,2),%r10
	lea	-320(%rbp,$num,2),%rbp	# future alloca(frame+2*num*8+256)
	sub	%r10,%r11
	mov	\$0,%r10
	cmovc	%r10,%r11
	sub	%r11,%rbp
.Lpwr_sp_done:
	and	\$-64,%rbp
	mov	%rsp,%r11
	sub	%rbp,%r11
	and	\$-4096,%r11
	lea	(%rbp,%r11),%rsp
	mov	(%rsp),%r10
	cmp	%rbp,%rsp
	ja	.Lpwr_page_walk
	jmp	.Lpwr_page_walk_done

.Lpwr_page_walk:
	lea	-4096(%rsp),%rsp
	mov	(%rsp),%r10
	cmp	%rbp,%rsp
	ja	.Lpwr_page_walk
.Lpwr_page_walk_done:

	mov	$num,%r10
	neg	$num

	##############################################################
	# Stack layout
	#
	# +0	saved $num, used in reduction section
	# +8	&t[2*$num], used in reduction section
	# +32	saved *n0
	# +40	saved %rsp
	# +48	t[2*$num]
	#
	mov	$n0,  32(%rsp)
	mov	%rax, 40(%rsp)		# save original %rsp
.cfi_cfa_expression	%rsp+40,deref,+8
.Lpower5_body:
	movq	$rptr,%xmm1		# save $rptr, used in sqr8x
	movq	$nptr,%xmm2		# save $nptr
	movq	%r10, %xmm3		# -$num, used in sqr8x
	movq	$bptr,%xmm4

	call	__bn_sqr8x_internal
	call	__bn_post4x_internal
	call	__bn_sqr8x_internal
	call	__bn_post4x_internal
	call	__bn_sqr8x_internal
	call	__bn_post4x_internal
	call	__bn_sqr8x_internal
	call	__bn_post4x_internal
	call	__bn_sqr8x_internal
	call	__bn_post4x_internal

	movq	%xmm2,$nptr
	movq	%xmm4,$bptr
	mov	$aptr,$rptr
	mov	40(%rsp),%rax
	lea	32(%rsp),$n0

	call	mul4x_internal

	mov	40(%rsp),%rsi		# restore %rsp
.cfi_def_cfa	%rsi,8
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
.Lpower5_epilogue:
	ret
.cfi_endproc
.size	bn_power5_nohw,.-bn_power5_nohw

.globl	bn_sqr8x_internal
.hidden	bn_sqr8x_internal
.type	bn_sqr8x_internal,\@abi-omnipotent
.align	32
bn_sqr8x_internal:
__bn_sqr8x_internal:
.cfi_startproc
	_CET_ENDBR
	##############################################################
	# Squaring part:
	#
	# a) multiply-n-add everything but a[i]*a[i];
	# b) shift result of a) by 1 to the left and accumulate
	#    a[i]*a[i] products;
	#
	##############################################################
	#                                                     a[1]a[0]
	#                                                 a[2]a[0]
	#                                             a[3]a[0]
	#                                             a[2]a[1]
	#                                         a[4]a[0]
	#                                         a[3]a[1]
	#                                     a[5]a[0]
	#                                     a[4]a[1]
	#                                     a[3]a[2]
	#                                 a[6]a[0]
	#                                 a[5]a[1]
	#                                 a[4]a[2]
	#                             a[7]a[0]
	#                             a[6]a[1]
	#                             a[5]a[2]
	#                             a[4]a[3]
	#                         a[7]a[1]
	#                         a[6]a[2]
	#                         a[5]a[3]
	#                     a[7]a[2]
	#                     a[6]a[3]
	#                     a[5]a[4]
	#                 a[7]a[3]
	#                 a[6]a[4]
	#             a[7]a[4]
	#             a[6]a[5]
	#         a[7]a[5]
	#     a[7]a[6]
	#                                                     a[1]a[0]
	#                                                 a[2]a[0]
	#                                             a[3]a[0]
	#                                         a[4]a[0]
	#                                     a[5]a[0]
	#                                 a[6]a[0]
	#                             a[7]a[0]
	#                                             a[2]a[1]
	#                                         a[3]a[1]
	#                                     a[4]a[1]
	#                                 a[5]a[1]
	#                             a[6]a[1]
	#                         a[7]a[1]
	#                                     a[3]a[2]
	#                                 a[4]a[2]
	#                             a[5]a[2]
	#                         a[6]a[2]
	#                     a[7]a[2]
	#                             a[4]a[3]
	#                         a[5]a[3]
	#                     a[6]a[3]
	#                 a[7]a[3]
	#                     a[5]a[4]
	#                 a[6]a[4]
	#             a[7]a[4]
	#             a[6]a[5]
	#         a[7]a[5]
	#     a[7]a[6]
	#                                                         a[0]a[0]
	#                                                 a[1]a[1]
	#                                         a[2]a[2]
	#                                 a[3]a[3]
	#                         a[4]a[4]
	#                 a[5]a[5]
	#         a[6]a[6]
	# a[7]a[7]

	lea	32(%r10),$i		# $i=-($num-32)
	lea	($aptr,$num),$aptr	# end of a[] buffer, ($aptr,$i)=&ap[2]

	mov	$num,$j			# $j=$num

					# comments apply to $num==8 case
	mov	-32($aptr,$i),$a0	# a[0]
	lea	48+8(%rsp,$num,2),$tptr	# end of tp[] buffer, &tp[2*$num]
	mov	-24($aptr,$i),%rax	# a[1]
	lea	-32($tptr,$i),$tptr	# end of tp[] window, &tp[2*$num-"$i"]
	mov	-16($aptr,$i),$ai	# a[2]
	mov	%rax,$a1

	mul	$a0			# a[1]*a[0]
	mov	%rax,$A0[0]		# a[1]*a[0]
	 mov	$ai,%rax		# a[2]
	mov	%rdx,$A0[1]
	mov	$A0[0],-24($tptr,$i)	# t[1]

	mul	$a0			# a[2]*a[0]
	add	%rax,$A0[1]
	 mov	$ai,%rax
	adc	\$0,%rdx
	mov	$A0[1],-16($tptr,$i)	# t[2]
	mov	%rdx,$A0[0]


	 mov	-8($aptr,$i),$ai	# a[3]
	mul	$a1			# a[2]*a[1]
	mov	%rax,$A1[0]		# a[2]*a[1]+t[3]
	 mov	$ai,%rax
	mov	%rdx,$A1[1]

	 lea	($i),$j
	mul	$a0			# a[3]*a[0]
	add	%rax,$A0[0]		# a[3]*a[0]+a[2]*a[1]+t[3]
	 mov	$ai,%rax
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	add	$A1[0],$A0[0]
	adc	\$0,$A0[1]
	mov	$A0[0],-8($tptr,$j)	# t[3]
	jmp	.Lsqr4x_1st

.align	32
.Lsqr4x_1st:
	 mov	($aptr,$j),$ai		# a[4]
	mul	$a1			# a[3]*a[1]
	add	%rax,$A1[1]		# a[3]*a[1]+t[4]
	 mov	$ai,%rax
	mov	%rdx,$A1[0]
	adc	\$0,$A1[0]

	mul	$a0			# a[4]*a[0]
	add	%rax,$A0[1]		# a[4]*a[0]+a[3]*a[1]+t[4]
	 mov	$ai,%rax		# a[3]
	 mov	8($aptr,$j),$ai		# a[5]
	mov	%rdx,$A0[0]
	adc	\$0,$A0[0]
	add	$A1[1],$A0[1]
	adc	\$0,$A0[0]


	mul	$a1			# a[4]*a[3]
	add	%rax,$A1[0]		# a[4]*a[3]+t[5]
	 mov	$ai,%rax
	 mov	$A0[1],($tptr,$j)	# t[4]
	mov	%rdx,$A1[1]
	adc	\$0,$A1[1]

	mul	$a0			# a[5]*a[2]
	add	%rax,$A0[0]		# a[5]*a[2]+a[4]*a[3]+t[5]
	 mov	$ai,%rax
	 mov	16($aptr,$j),$ai	# a[6]
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	add	$A1[0],$A0[0]
	adc	\$0,$A0[1]

	mul	$a1			# a[5]*a[3]
	add	%rax,$A1[1]		# a[5]*a[3]+t[6]
	 mov	$ai,%rax
	 mov	$A0[0],8($tptr,$j)	# t[5]
	mov	%rdx,$A1[0]
	adc	\$0,$A1[0]

	mul	$a0			# a[6]*a[2]
	add	%rax,$A0[1]		# a[6]*a[2]+a[5]*a[3]+t[6]
	 mov	$ai,%rax		# a[3]
	 mov	24($aptr,$j),$ai	# a[7]
	mov	%rdx,$A0[0]
	adc	\$0,$A0[0]
	add	$A1[1],$A0[1]
	adc	\$0,$A0[0]


	mul	$a1			# a[6]*a[5]
	add	%rax,$A1[0]		# a[6]*a[5]+t[7]
	 mov	$ai,%rax
	 mov	$A0[1],16($tptr,$j)	# t[6]
	mov	%rdx,$A1[1]
	adc	\$0,$A1[1]
	 lea	32($j),$j

	mul	$a0			# a[7]*a[4]
	add	%rax,$A0[0]		# a[7]*a[4]+a[6]*a[5]+t[6]
	 mov	$ai,%rax
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	add	$A1[0],$A0[0]
	adc	\$0,$A0[1]
	mov	$A0[0],-8($tptr,$j)	# t[7]

	cmp	\$0,$j
	jne	.Lsqr4x_1st

	mul	$a1			# a[7]*a[5]
	add	%rax,$A1[1]
	lea	16($i),$i
	adc	\$0,%rdx
	add	$A0[1],$A1[1]
	adc	\$0,%rdx

	mov	$A1[1],($tptr)		# t[8]
	mov	%rdx,$A1[0]
	mov	%rdx,8($tptr)		# t[9]
	jmp	.Lsqr4x_outer

.align	32
.Lsqr4x_outer:				# comments apply to $num==6 case
	mov	-32($aptr,$i),$a0	# a[0]
	lea	48+8(%rsp,$num,2),$tptr	# end of tp[] buffer, &tp[2*$num]
	mov	-24($aptr,$i),%rax	# a[1]
	lea	-32($tptr,$i),$tptr	# end of tp[] window, &tp[2*$num-"$i"]
	mov	-16($aptr,$i),$ai	# a[2]
	mov	%rax,$a1

	mul	$a0			# a[1]*a[0]
	mov	-24($tptr,$i),$A0[0]	# t[1]
	add	%rax,$A0[0]		# a[1]*a[0]+t[1]
	 mov	$ai,%rax		# a[2]
	adc	\$0,%rdx
	mov	$A0[0],-24($tptr,$i)	# t[1]
	mov	%rdx,$A0[1]

	mul	$a0			# a[2]*a[0]
	add	%rax,$A0[1]
	 mov	$ai,%rax
	adc	\$0,%rdx
	add	-16($tptr,$i),$A0[1]	# a[2]*a[0]+t[2]
	mov	%rdx,$A0[0]
	adc	\$0,$A0[0]
	mov	$A0[1],-16($tptr,$i)	# t[2]

	xor	$A1[0],$A1[0]

	 mov	-8($aptr,$i),$ai	# a[3]
	mul	$a1			# a[2]*a[1]
	add	%rax,$A1[0]		# a[2]*a[1]+t[3]
	 mov	$ai,%rax
	adc	\$0,%rdx
	add	-8($tptr,$i),$A1[0]
	mov	%rdx,$A1[1]
	adc	\$0,$A1[1]

	mul	$a0			# a[3]*a[0]
	add	%rax,$A0[0]		# a[3]*a[0]+a[2]*a[1]+t[3]
	 mov	$ai,%rax
	adc	\$0,%rdx
	add	$A1[0],$A0[0]
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	mov	$A0[0],-8($tptr,$i)	# t[3]

	lea	($i),$j
	jmp	.Lsqr4x_inner

.align	32
.Lsqr4x_inner:
	 mov	($aptr,$j),$ai		# a[4]
	mul	$a1			# a[3]*a[1]
	add	%rax,$A1[1]		# a[3]*a[1]+t[4]
	 mov	$ai,%rax
	mov	%rdx,$A1[0]
	adc	\$0,$A1[0]
	add	($tptr,$j),$A1[1]
	adc	\$0,$A1[0]

	.byte	0x67
	mul	$a0			# a[4]*a[0]
	add	%rax,$A0[1]		# a[4]*a[0]+a[3]*a[1]+t[4]
	 mov	$ai,%rax		# a[3]
	 mov	8($aptr,$j),$ai		# a[5]
	mov	%rdx,$A0[0]
	adc	\$0,$A0[0]
	add	$A1[1],$A0[1]
	adc	\$0,$A0[0]

	mul	$a1			# a[4]*a[3]
	add	%rax,$A1[0]		# a[4]*a[3]+t[5]
	mov	$A0[1],($tptr,$j)	# t[4]
	 mov	$ai,%rax
	mov	%rdx,$A1[1]
	adc	\$0,$A1[1]
	add	8($tptr,$j),$A1[0]
	lea	16($j),$j		# j++
	adc	\$0,$A1[1]

	mul	$a0			# a[5]*a[2]
	add	%rax,$A0[0]		# a[5]*a[2]+a[4]*a[3]+t[5]
	 mov	$ai,%rax
	adc	\$0,%rdx
	add	$A1[0],$A0[0]
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	mov	$A0[0],-8($tptr,$j)	# t[5], "preloaded t[1]" below

	cmp	\$0,$j
	jne	.Lsqr4x_inner

	.byte	0x67
	mul	$a1			# a[5]*a[3]
	add	%rax,$A1[1]
	adc	\$0,%rdx
	add	$A0[1],$A1[1]
	adc	\$0,%rdx

	mov	$A1[1],($tptr)		# t[6], "preloaded t[2]" below
	mov	%rdx,$A1[0]
	mov	%rdx,8($tptr)		# t[7], "preloaded t[3]" below

	add	\$16,$i
	jnz	.Lsqr4x_outer

					# comments apply to $num==4 case
	mov	-32($aptr),$a0		# a[0]
	lea	48+8(%rsp,$num,2),$tptr	# end of tp[] buffer, &tp[2*$num]
	mov	-24($aptr),%rax		# a[1]
	lea	-32($tptr,$i),$tptr	# end of tp[] window, &tp[2*$num-"$i"]
	mov	-16($aptr),$ai		# a[2]
	mov	%rax,$a1

	mul	$a0			# a[1]*a[0]
	add	%rax,$A0[0]		# a[1]*a[0]+t[1], preloaded t[1]
	 mov	$ai,%rax		# a[2]
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]

	mul	$a0			# a[2]*a[0]
	add	%rax,$A0[1]
	 mov	$ai,%rax
	 mov	$A0[0],-24($tptr)	# t[1]
	mov	%rdx,$A0[0]
	adc	\$0,$A0[0]
	add	$A1[1],$A0[1]		# a[2]*a[0]+t[2], preloaded t[2]
	 mov	-8($aptr),$ai		# a[3]
	adc	\$0,$A0[0]

	mul	$a1			# a[2]*a[1]
	add	%rax,$A1[0]		# a[2]*a[1]+t[3], preloaded t[3]
	 mov	$ai,%rax
	 mov	$A0[1],-16($tptr)	# t[2]
	mov	%rdx,$A1[1]
	adc	\$0,$A1[1]

	mul	$a0			# a[3]*a[0]
	add	%rax,$A0[0]		# a[3]*a[0]+a[2]*a[1]+t[3]
	 mov	$ai,%rax
	mov	%rdx,$A0[1]
	adc	\$0,$A0[1]
	add	$A1[0],$A0[0]
	adc	\$0,$A0[1]
	mov	$A0[0],-8($tptr)	# t[3]

	mul	$a1			# a[3]*a[1]
	add	%rax,$A1[1]
	 mov	-16($aptr),%rax		# a[2]
	adc	\$0,%rdx
	add	$A0[1],$A1[1]
	adc	\$0,%rdx

	mov	$A1[1],($tptr)		# t[4]
	mov	%rdx,$A1[0]
	mov	%rdx,8($tptr)		# t[5]

	mul	$ai			# a[2]*a[3]
___
{
my ($shift,$carry)=($a0,$a1);
my @S=(@A1,$ai,$n0);
$code.=<<___;
	 add	\$16,$i
	 xor	$shift,$shift
	 sub	$num,$i			# $i=16-$num
	 xor	$carry,$carry

	add	$A1[0],%rax		# t[5]
	adc	\$0,%rdx
	mov	%rax,8($tptr)		# t[5]
	mov	%rdx,16($tptr)		# t[6]
	mov	$carry,24($tptr)	# t[7]

	 mov	-16($aptr,$i),%rax	# a[0]
	lea	48+8(%rsp),$tptr
	 xor	$A0[0],$A0[0]		# t[0]
	 mov	8($tptr),$A0[1]		# t[1]

	lea	($shift,$A0[0],2),$S[0]	# t[2*i]<<1 | shift
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[1]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[1]		# | t[2*i]>>63
	 mov	16($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	24($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[0]
	 mov	-8($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[0],($tptr)
	adc	%rdx,$S[1]

	lea	($shift,$A0[0],2),$S[2]	# t[2*i]<<1 | shift
	 mov	$S[1],8($tptr)
	 sbb	$carry,$carry		# mov cf,$carry
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[3]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[3]		# | t[2*i]>>63
	 mov	32($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	40($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[2]
	 mov	0($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[2],16($tptr)
	adc	%rdx,$S[3]
	lea	16($i),$i
	mov	$S[3],24($tptr)
	sbb	$carry,$carry		# mov cf,$carry
	lea	64($tptr),$tptr
	jmp	.Lsqr4x_shift_n_add

.align	32
.Lsqr4x_shift_n_add:
	lea	($shift,$A0[0],2),$S[0]	# t[2*i]<<1 | shift
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[1]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[1]		# | t[2*i]>>63
	 mov	-16($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	-8($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[0]
	 mov	-8($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[0],-32($tptr)
	adc	%rdx,$S[1]

	lea	($shift,$A0[0],2),$S[2]	# t[2*i]<<1 | shift
	 mov	$S[1],-24($tptr)
	 sbb	$carry,$carry		# mov cf,$carry
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[3]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[3]		# | t[2*i]>>63
	 mov	0($tptr),$A0[0]		# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	8($tptr),$A0[1]		# t[2*i+2+1]	# prefetch
	adc	%rax,$S[2]
	 mov	0($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[2],-16($tptr)
	adc	%rdx,$S[3]

	lea	($shift,$A0[0],2),$S[0]	# t[2*i]<<1 | shift
	 mov	$S[3],-8($tptr)
	 sbb	$carry,$carry		# mov cf,$carry
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[1]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[1]		# | t[2*i]>>63
	 mov	16($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	24($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[0]
	 mov	8($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[0],0($tptr)
	adc	%rdx,$S[1]

	lea	($shift,$A0[0],2),$S[2]	# t[2*i]<<1 | shift
	 mov	$S[1],8($tptr)
	 sbb	$carry,$carry		# mov cf,$carry
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[3]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[3]		# | t[2*i]>>63
	 mov	32($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	40($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[2]
	 mov	16($aptr,$i),%rax	# a[i+1]	# prefetch
	mov	$S[2],16($tptr)
	adc	%rdx,$S[3]
	mov	$S[3],24($tptr)
	sbb	$carry,$carry		# mov cf,$carry
	lea	64($tptr),$tptr
	add	\$32,$i
	jnz	.Lsqr4x_shift_n_add

	lea	($shift,$A0[0],2),$S[0]	# t[2*i]<<1 | shift
	.byte	0x67
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[1]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[1]		# | t[2*i]>>63
	 mov	-16($tptr),$A0[0]	# t[2*i+2]	# prefetch
	mov	$A0[1],$shift		# shift=t[2*i+1]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	 mov	-8($tptr),$A0[1]	# t[2*i+2+1]	# prefetch
	adc	%rax,$S[0]
	 mov	-8($aptr),%rax		# a[i+1]	# prefetch
	mov	$S[0],-32($tptr)
	adc	%rdx,$S[1]

	lea	($shift,$A0[0],2),$S[2]	# t[2*i]<<1|shift
	 mov	$S[1],-24($tptr)
	 sbb	$carry,$carry		# mov cf,$carry
	shr	\$63,$A0[0]
	lea	($j,$A0[1],2),$S[3]	# t[2*i+1]<<1 |
	shr	\$63,$A0[1]
	or	$A0[0],$S[3]		# | t[2*i]>>63
	mul	%rax			# a[i]*a[i]
	neg	$carry			# mov $carry,cf
	adc	%rax,$S[2]
	adc	%rdx,$S[3]
	mov	$S[2],-16($tptr)
	mov	$S[3],-8($tptr)
___
}
######################################################################
# Montgomery reduction part, "word-by-word" algorithm.
#
# This new path is inspired by multiple submissions from Intel, by
# Shay Gueron, Vlad Krasnov, Erdinc Ozturk, James Guilford,
# Vinodh Gopal...
{
my ($nptr,$tptr,$carry,$m0)=("%rbp","%rdi","%rsi","%rbx");

$code.=<<___;
	movq	%xmm2,$nptr
__bn_sqr8x_reduction:
	xor	%rax,%rax
	lea	($nptr,$num),%rcx	# end of n[]
	lea	48+8(%rsp,$num,2),%rdx	# end of t[] buffer
	mov	%rcx,0+8(%rsp)
	lea	48+8(%rsp,$num),$tptr	# end of initial t[] window
	mov	%rdx,8+8(%rsp)
	neg	$num
	jmp	.L8x_reduction_loop

.align	32
.L8x_reduction_loop:
	lea	($tptr,$num),$tptr	# start of current t[] window
	.byte	0x66
	mov	8*0($tptr),$m0
	mov	8*1($tptr),%r9
	mov	8*2($tptr),%r10
	mov	8*3($tptr),%r11
	mov	8*4($tptr),%r12
	mov	8*5($tptr),%r13
	mov	8*6($tptr),%r14
	mov	8*7($tptr),%r15
	mov	%rax,(%rdx)		# store top-most carry bit
	lea	8*8($tptr),$tptr

	.byte	0x67
	mov	$m0,%r8
	imulq	32+8(%rsp),$m0		# n0*a[0]
	mov	8*0($nptr),%rax		# n[0]
	mov	\$8,%ecx
	jmp	.L8x_reduce

.align	32
.L8x_reduce:
	mulq	$m0
	 mov	8*1($nptr),%rax		# n[1]
	neg	%r8
	mov	%rdx,%r8
	adc	\$0,%r8

	mulq	$m0
	add	%rax,%r9
	 mov	8*2($nptr),%rax
	adc	\$0,%rdx
	add	%r9,%r8
	 mov	$m0,48-8+8(%rsp,%rcx,8)	# put aside n0*a[i]
	mov	%rdx,%r9
	adc	\$0,%r9

	mulq	$m0
	add	%rax,%r10
	 mov	8*3($nptr),%rax
	adc	\$0,%rdx
	add	%r10,%r9
	 mov	32+8(%rsp),$carry	# pull n0, borrow $carry
	mov	%rdx,%r10
	adc	\$0,%r10

	mulq	$m0
	add	%rax,%r11
	 mov	8*4($nptr),%rax
	adc	\$0,%rdx
	 imulq	%r8,$carry		# modulo-scheduled
	add	%r11,%r10
	mov	%rdx,%r11
	adc	\$0,%r11

	mulq	$m0
	add	%rax,%r12
	 mov	8*5($nptr),%rax
	adc	\$0,%rdx
	add	%r12,%r11
	mov	%rdx,%r12
	adc	\$0,%r12

	mulq	$m0
	add	%rax,%r13
	 mov	8*6($nptr),%rax
	adc	\$0,%rdx
	add	%r13,%r12
	mov	%rdx,%r13
	adc	\$0,%r13

	mulq	$m0
	add	%rax,%r14
	 mov	8*7($nptr),%rax
	adc	\$0,%rdx
	add	%r14,%r13
	mov	%rdx,%r14
	adc	\$0,%r14

	mulq	$m0
	 mov	$carry,$m0		# n0*a[i]
	add	%rax,%r15
	 mov	8*0($nptr),%rax		# n[0]
	adc	\$0,%rdx
	add	%r15,%r14
	mov	%rdx,%r15
	adc	\$0,%r15

	dec	%ecx
	jnz	.L8x_reduce

	lea	8*8($nptr),$nptr
	xor	%rax,%rax
	mov	8+8(%rsp),%rdx		# pull end of t[]
	cmp	0+8(%rsp),$nptr		# end of n[]?
	jae	.L8x_no_tail

	.byte	0x66
	add	8*0($tptr),%r8
	adc	8*1($tptr),%r9
	adc	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	sbb	$carry,$carry		# top carry

	mov	48+56+8(%rsp),$m0	# pull n0*a[0]
	mov	\$8,%ecx
	mov	8*0($nptr),%rax
	jmp	.L8x_tail

.align	32
.L8x_tail:
	mulq	$m0
	add	%rax,%r8
	 mov	8*1($nptr),%rax
	 mov	%r8,($tptr)		# save result
	mov	%rdx,%r8
	adc	\$0,%r8

	mulq	$m0
	add	%rax,%r9
	 mov	8*2($nptr),%rax
	adc	\$0,%rdx
	add	%r9,%r8
	 lea	8($tptr),$tptr		# $tptr++
	mov	%rdx,%r9
	adc	\$0,%r9

	mulq	$m0
	add	%rax,%r10
	 mov	8*3($nptr),%rax
	adc	\$0,%rdx
	add	%r10,%r9
	mov	%rdx,%r10
	adc	\$0,%r10

	mulq	$m0
	add	%rax,%r11
	 mov	8*4($nptr),%rax
	adc	\$0,%rdx
	add	%r11,%r10
	mov	%rdx,%r11
	adc	\$0,%r11

	mulq	$m0
	add	%rax,%r12
	 mov	8*5($nptr),%rax
	adc	\$0,%rdx
	add	%r12,%r11
	mov	%rdx,%r12
	adc	\$0,%r12

	mulq	$m0
	add	%rax,%r13
	 mov	8*6($nptr),%rax
	adc	\$0,%rdx
	add	%r13,%r12
	mov	%rdx,%r13
	adc	\$0,%r13

	mulq	$m0
	add	%rax,%r14
	 mov	8*7($nptr),%rax
	adc	\$0,%rdx
	add	%r14,%r13
	mov	%rdx,%r14
	adc	\$0,%r14

	mulq	$m0
	 mov	48-16+8(%rsp,%rcx,8),$m0# pull n0*a[i]
	add	%rax,%r15
	adc	\$0,%rdx
	add	%r15,%r14
	 mov	8*0($nptr),%rax		# pull n[0]
	mov	%rdx,%r15
	adc	\$0,%r15

	dec	%ecx
	jnz	.L8x_tail

	lea	8*8($nptr),$nptr
	mov	8+8(%rsp),%rdx		# pull end of t[]
	cmp	0+8(%rsp),$nptr		# end of n[]?
	jae	.L8x_tail_done		# break out of loop

	 mov	48+56+8(%rsp),$m0	# pull n0*a[0]
	neg	$carry
	 mov	8*0($nptr),%rax		# pull n[0]
	adc	8*0($tptr),%r8
	adc	8*1($tptr),%r9
	adc	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	sbb	$carry,$carry		# top carry

	mov	\$8,%ecx
	jmp	.L8x_tail

.align	32
.L8x_tail_done:
	xor	%rax,%rax
	add	(%rdx),%r8		# can this overflow?
	adc	\$0,%r9
	adc	\$0,%r10
	adc	\$0,%r11
	adc	\$0,%r12
	adc	\$0,%r13
	adc	\$0,%r14
	adc	\$0,%r15
	adc	\$0,%rax

	neg	$carry
.L8x_no_tail:
	adc	8*0($tptr),%r8
	adc	8*1($tptr),%r9
	adc	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	adc	\$0,%rax		# top-most carry
	 mov	-8($nptr),%rcx		# np[num-1]
	 xor	$carry,$carry

	movq	%xmm2,$nptr		# restore $nptr

	mov	%r8,8*0($tptr)		# store top 512 bits
	mov	%r9,8*1($tptr)
	 movq	%xmm3,$num		# $num is %r9, can't be moved upwards
	mov	
	mov	
	mov	
	mov	
	mov	
	mov	
	lea	8*8($tptr),$tptr

	cmp	
	jb	.L8x_reduction_loop
	ret
.cfi_endproc
.size	bn_sqr8x_internal,.-bn_sqr8x_internal
___
}
##############################################################
# Post-condition, 4x unrolled
#
{
my ($tptr,$nptr)=("%rbx","%rbp");
$code.=<<___;
.type	__bn_post4x_internal,\@abi-omnipotent
.align	32
__bn_post4x_internal:
.cfi_startproc
	mov	8*0($nptr), 
	lea	( 
	mov	$num, 
	movq	
	neg	
	movq	
	sar	\$3+2, 
	dec	
	xor	
	mov	8*1($nptr), 
	mov	8*2($nptr), 
	mov	8*3($nptr), 
	jmp	.Lsqr4x_sub_entry

.align	16
.Lsqr4x_sub:
	mov	8*0($nptr), 
	mov	8*1($nptr), 
	mov	8*2($nptr), 
	mov	8*3($nptr), 
.Lsqr4x_sub_entry:
	lea	8*4($nptr),$nptr
	not	
	not	
	not	
	not	
	and	
	and	
	and	
	and	

	neg	
	adc	8*0($tptr), 
	adc	8*1($tptr), 
	adc	8*2($tptr), 
	adc	8*3($tptr), 
	mov	
	lea	8*4($tptr),$tptr
	mov	
	sbb	
	mov	
	mov	
	lea	8*4($rptr),$rptr

	inc	
	jnz	.Lsqr4x_sub

	mov	$num, 
	neg	$num			# restore $num
	ret
.cfi_endproc
.size	__bn_post4x_internal,.-__bn_post4x_internal
___
}
}}}

if ($addx) {{{
my $bp="%rdx";	# restore original value

$code.=<<___;
.globl	bn_mulx4x_mont_gather5
.type	bn_mulx4x_mont_gather5,\@function,6
.align	32
bn_mulx4x_mont_gather5:
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

	# num is declared as an int, a 32-bit parameter, so the upper half is
	# undefined. It is important that this write to ${num}, which zeros the
	# upper half, predates the first access.
	shl	\$3,${num}d		# convert $num to bytes
	lea	($num,$num,2), 
	neg	$num			# -$num
	mov	($n0),$n0		# *n0

	##############################################################
	# Ensure that stack frame doesn't alias with $rptr+3*$num
	# modulo 4096, which covers ret[num], am[num] and n[num]
	# (see bn_exp.c). This is done to allow memory disambiguation
	# logic do its magic. [Extra [num] is allocated in order
	# to align with bn_power5's frame, which is cleansed after
	# completing exponentiation. Extra 256 bytes is for power mask
	# calculated from 7th argument, the index.]
	#
	lea	-320( 
	mov	
	sub	$rp, 
	and	\$4095, 
	cmp	
	jb	.Lmulx4xsp_alt
	sub	
	lea	-320( 
	jmp	.Lmulx4xsp_done

.Lmulx4xsp_alt:
	lea	4096-320(,$num,2), 
	lea	-320( 
	sub	
	mov	\$0, 
	cmovc	
	sub	
.Lmulx4xsp_done:
	and	\$-64, 
	mov	
	sub	
	and	\$-4096, 
	lea	( 
	mov	( 
	cmp	
	ja	.Lmulx4x_page_walk
	jmp	.Lmulx4x_page_walk_done

.Lmulx4x_page_walk:
	lea	-4096( 
	mov	( 
	cmp	
	ja	.Lmulx4x_page_walk
.Lmulx4x_page_walk_done:

	##############################################################
	# Stack layout
	# +0	-num
	# +8	off-loaded &b[i]
	# +16	end of b[num]
	# +24	inner counter
	# +32	saved n0
	# +40	saved 
	# +48
	# +56	saved rp
	# +64	tmp[num+1]
	#
	mov	$n0, 32( 
	mov	
.cfi_cfa_expression	
.Lmulx4x_body:
	call	mulx4x_internal

	mov	40( 
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
.Lmulx4x_epilogue:
	ret
.cfi_endproc
.size	bn_mulx4x_mont_gather5,.-bn_mulx4x_mont_gather5

.type	mulx4x_internal,\@abi-omnipotent
.align	32
mulx4x_internal:
.cfi_startproc
	mov	$num,8( 
	mov	$num, 
	neg	$num			# restore $num
	shl	\$5,$num
	neg	
	lea	128($bp,$num), 
	shr	\$5+5,$num
	movd	`($win64?56:8)`( 
	sub	\$1,$num
	lea	.Linc( 
	mov	
	mov	$num,24+8( 
	mov	$rp, 56+8( 
___
my ($aptr, $bptr, $nptr, $tptr, $mi,  $bi,  $zero, $num)=
   ("%rsi","%rdi","%rcx","%rbx","%r8","%r9","%rbp","%rax");
my $rptr=$bptr;
my $STRIDE=2**5*8;		# 5 is "window size"
$code.=<<___;
	movdqa	0( 
	movdqa	16( 
	lea	88-112( 
	lea	128($bp),$bptr		# size optimization

	pshufd	\$0, 
	movdqa	
	.byte	0x67
	movdqa	
___
########################################################################
# Calculate masks by comparing 0..31 to $idx and save result to stack.
#
# We compute sixteen 16-byte masks and store them on the stack. Mask i is stored
# in `16*i - 128`( 
# idx == 2*i + 1 in its lower and upper halves, respectively. Mask calculations
# are scheduled in groups of four.
$code.=<<___;
	.byte	0x67
	paddd	
	pcmpeqd	
	movdqa	
___
for($i=0;$i<$STRIDE/16-4;$i+=4) {
$code.=<<___;
	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	
___
}
$code.=<<___;				# last iteration can be optimized
	.byte	0x67
	paddd	
	pcmpeqd	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	

	pcmpeqd	
	movdqa	

	pand	`16*($i+0)-128`($bptr), 
	pand	`16*($i+1)-128`($bptr), 
	pand	`16*($i+2)-128`($bptr), 
	movdqa	
	pand	`16*($i+3)-128`($bptr), 
	por	
	por	
___
for($i=0;$i<$STRIDE/16-4;$i+=4) {
$code.=<<___;
	movdqa	`16*($i+0)-128`($bptr), 
	movdqa	`16*($i+1)-128`($bptr), 
	movdqa	`16*($i+2)-128`($bptr), 
	pand	`16*($i+0)+112`( 
	movdqa	`16*($i+3)-128`($bptr), 
	pand	`16*($i+1)+112`( 
	por	
	pand	`16*($i+2)+112`( 
	por	
	pand	`16*($i+3)+112`( 
	por	
	por	
___
}
$code.=<<___;
	pxor	
	# Combine the upper and lower halves of 
	pshufd	\$0x4e, 
	por	
	lea	$STRIDE($bptr),$bptr
	movq	
	lea	64+8*4+8( 

	mov	
	mulx	0*8($aptr),$mi, 
	mulx	1*8($aptr), 
	add	
	mulx	2*8($aptr), 
	adc	
	adc	\$0, 
	mulx	3*8($aptr), 

	mov	$mi, 
	imulq	32+8( 
	xor	$zero,$zero		# cf=0, of=0
	mov	$mi, 

	mov	$bptr,8+8( 

	lea	4*8($aptr),$aptr
	adcx	
	adcx	$zero, 

	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	mulx	2*8($nptr), 
	mov	24+8( 
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

	mov	8( 
	adc	$zero, 
	lea	($aptr,$num),$aptr	# rewind $aptr
	add	
	mov	8+8( 
	adc	$zero,$zero		# top-most carry
	mov	
	jmp	.Lmulx4x_outer

.align	32
.Lmulx4x_outer:
	lea	16-256($tptr), 
	pxor	
	.byte	0x67,0x67
	pxor	
___
for($i=0;$i<$STRIDE/16;$i+=4) {
$code.=<<___;
	movdqa	`16*($i+0)-128`($bptr), 
	movdqa	`16*($i+1)-128`($bptr), 
	movdqa	`16*($i+2)-128`($bptr), 
	pand	`16*($i+0)+256`( 
	movdqa	`16*($i+3)-128`($bptr), 
	pand	`16*($i+1)+256`( 
	por	
	pand	`16*($i+2)+256`( 
	por	
	pand	`16*($i+3)+256`( 
	por	
	por	
___
}
$code.=<<___;
	por	
	# Combine the upper and lower halves of 
	pshufd	\$0x4e, 
	por	
	lea	$STRIDE($bptr),$bptr
	movq	

	mov	$zero,($tptr)		# save top-most carry
	lea	4*8($tptr,$num),$tptr	# rewind $tptr
	mulx	0*8($aptr),$mi, 
	xor	$zero,$zero		# cf=0, of=0
	mov	
	mulx	1*8($aptr), 
	adox	-4*8($tptr),$mi		# +t[0]
	adcx	
	mulx	2*8($aptr), 
	adox	-3*8($tptr), 
	adcx	
	mulx	3*8($aptr), 
	adox	-2*8($tptr), 
	adcx	
	lea	($nptr,$num),$nptr	# rewind $nptr
	lea	4*8($aptr),$aptr
	adox	-1*8($tptr), 
	adcx	$zero, 
	adox	$zero, 

	mov	$mi, 
	imulq	32+8( 

	mov	$mi, 
	xor	$zero,$zero		# cf=0, of=0
	mov	$bptr,8+8( 

	mulx	0*8($nptr), 
	adcx	
	adox	
	mulx	1*8($nptr), 
	adcx	
	adox	
	mulx	2*8($nptr), 
	adcx	
	adox	
	mulx	3*8($nptr), 
	 mov	$bi, 
	mov	24+8( 
	mov	
	adcx	
	mov	
	adox	$zero, 
	mov	
	lea	4*8($nptr),$nptr
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
	mov	
	mulx	3*8($nptr), 
	 mov	$bi, 
	lea	4*8($nptr),$nptr
	mov	
	adcx	
	adox	$zero, 
	mov	

	dec	$bptr			# of=0, pass cf
	jnz	.Lmulx4x_inner

	mov	0+8( 
	adc	$zero, 
	sub	0*8($tptr),$bptr	# pull top-most carry to 
	mov	8+8( 
	mov	16+8( 
	adc	
	lea	($aptr,$num),$aptr	# rewind $aptr
	adc	$zero,$zero		# top-most carry
	mov	

	cmp	
	jb	.Lmulx4x_outer

	mov	-8($nptr), 
	mov	$zero, 
	mov	($nptr,$num), 
	lea	($nptr,$num), 
	mov	$num, 
	lea	($tptr,$num), 
	xor	
	xor	
	sub	
	adc	
	or	
	sar	\$3+2, 
	sub	
	mov	56+8( 
	dec	
	mov	8*1( 
	xor	
	mov	8*2( 
	mov	8*3( 
	jmp	.Lsqrx4x_sub_entry	# common post-condition
.cfi_endproc
.size	mulx4x_internal,.-mulx4x_internal
___
}{
######################################################################
# void bn_powerx5(
my $rptr="%rdi";	# BN_ULONG *rptr,
my $aptr="%rsi";	# const BN_ULONG *aptr,
my $bptr="%rdx";	# const void *table,
my $nptr="%rcx";	# const BN_ULONG *nptr,
my $n0  ="%r8";		# const BN_ULONG *n0);
my $num ="%r9";		# int num, has to be divisible by 8
			# int pwr);

my ($i,$j,$tptr)=("%rbp","%rcx",$rptr);
my @A0=("%r10","%r11");
my @A1=("%r12","%r13");
my ($a0,$a1,$ai)=("%r14","%r15","%rbx");

$code.=<<___;
.globl	bn_powerx5
.type	bn_powerx5,\@function,6
.align	32
bn_powerx5:
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
.Lpowerx5_prologue:

	# num is declared as an int, a 32-bit parameter, so the upper half is
	# undefined. It is important that this write to ${num}, which zeros the
	# upper half, predates the first access.
	shl	\$3,${num}d		# convert $num to bytes
	lea	($num,$num,2), 
	neg	$num
	mov	($n0),$n0		# *n0

	##############################################################
	# Ensure that stack frame doesn't alias with $rptr+3*$num
	# modulo 4096, which covers ret[num], am[num] and n[num]
	# (see bn_exp.c). This is done to allow memory disambiguation
	# logic do its magic. [Extra 256 bytes is for power mask
	# calculated from 7th argument, the index.]
	#
	lea	-320(%rsp,$num,2),%r11
	mov	%rsp,%rbp
	sub	$rptr,%r11
	and	\$4095,%r11
	cmp	%r11,%r10
	jb	.Lpwrx_sp_alt
	sub	%r11,%rbp		# align with $aptr
	lea	-320(%rbp,$num,2),%rbp	# future alloca(frame+2*$num*8+256)
	jmp	.Lpwrx_sp_done

.align	32
.Lpwrx_sp_alt:
	lea	4096-320(,$num,2),%r10
	lea	-320(%rbp,$num,2),%rbp	# alloca(frame+2*$num*8+256)
	sub	%r10,%r11
	mov	\$0,%r10
	cmovc	%r10,%r11
	sub	%r11,%rbp
.Lpwrx_sp_done:
	and	\$-64,%rbp
	mov	%rsp,%r11
	sub	%rbp,%r11
	and	\$-4096,%r11
	lea	(%rbp,%r11),%rsp
	mov	(%rsp),%r10
	cmp	%rbp,%rsp
	ja	.Lpwrx_page_walk
	jmp	.Lpwrx_page_walk_done

.Lpwrx_page_walk:
	lea	-4096(%rsp),%rsp
	mov	(%rsp),%r10
	cmp	%rbp,%rsp
	ja	.Lpwrx_page_walk
.Lpwrx_page_walk_done:

	mov	$num,%r10
	neg	$num

	##############################################################
	# Stack layout
	#
	# +0	saved $num, used in reduction section
	# +8	&t[2*$num], used in reduction section
	# +16	intermediate carry bit
	# +24	top-most carry bit, used in reduction section
	# +32	saved *n0
	# +40	saved %rsp
	# +48	t[2*$num]
	#
	pxor	%xmm0,%xmm0
	movq	$rptr,%xmm1		# save $rptr
	movq	$nptr,%xmm2		# save $nptr
	movq	%r10, %xmm3		# -$num
	movq	$bptr,%xmm4
	mov	$n0,  32(%rsp)
	mov	%rax, 40(%rsp)		# save original %rsp
.cfi_cfa_expression	%rsp+40,deref,+8
.Lpowerx5_body:

	call	__bn_sqrx8x_internal
	call	__bn_postx4x_internal
	call	__bn_sqrx8x_internal
	call	__bn_postx4x_internal
	call	__bn_sqrx8x_internal
	call	__bn_postx4x_internal
	call	__bn_sqrx8x_internal
	call	__bn_postx4x_internal
	call	__bn_sqrx8x_internal
	call	__bn_postx4x_internal

	mov	%r10,$num		# -num
	mov	$aptr,$rptr
	movq	%xmm2,$nptr
	movq	%xmm4,$bptr
	mov	40(%rsp),%rax

	call	mulx4x_internal

	mov	40(%rsp),%rsi		# restore %rsp
.cfi_def_cfa	%rsi,8
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
.Lpowerx5_epilogue:
	ret
.cfi_endproc
.size	bn_powerx5,.-bn_powerx5

.globl	bn_sqrx8x_internal
.hidden	bn_sqrx8x_internal
.type	bn_sqrx8x_internal,\@abi-omnipotent
.align	32
bn_sqrx8x_internal:
__bn_sqrx8x_internal:
.cfi_startproc
	_CET_ENDBR
	##################################################################
	# Squaring part:
	#
	# a) multiply-n-add everything but a[i]*a[i];
	# b) shift result of a) by 1 to the left and accumulate
	#    a[i]*a[i] products;
	#
	##################################################################
	# a[7]a[7]a[6]a[6]a[5]a[5]a[4]a[4]a[3]a[3]a[2]a[2]a[1]a[1]a[0]a[0]
	#                                                     a[1]a[0]
	#                                                 a[2]a[0]
	#                                             a[3]a[0]
	#                                             a[2]a[1]
	#                                         a[3]a[1]
	#                                     a[3]a[2]
	#
	#                                         a[4]a[0]
	#                                     a[5]a[0]
	#                                 a[6]a[0]
	#                             a[7]a[0]
	#                                     a[4]a[1]
	#                                 a[5]a[1]
	#                             a[6]a[1]
	#                         a[7]a[1]
	#                                 a[4]a[2]
	#                             a[5]a[2]
	#                         a[6]a[2]
	#                     a[7]a[2]
	#                             a[4]a[3]
	#                         a[5]a[3]
	#                     a[6]a[3]
	#                 a[7]a[3]
	#
	#                     a[5]a[4]
	#                 a[6]a[4]
	#             a[7]a[4]
	#             a[6]a[5]
	#         a[7]a[5]
	#     a[7]a[6]
	# a[7]a[7]a[6]a[6]a[5]a[5]a[4]a[4]a[3]a[3]a[2]a[2]a[1]a[1]a[0]a[0]
___
{
my ($zero,$carry)=("%rbp","%rcx");
my $aaptr=$zero;
$code.=<<___;
	lea	48+8(%rsp),$tptr
	lea	($aptr,$num),$aaptr
	mov	$num,0+8(%rsp)			# save $num
	mov	$aaptr,8+8(%rsp)		# save end of $aptr
	jmp	.Lsqr8x_zero_start

.align	32
.byte	0x66,0x66,0x66,0x2e,0x0f,0x1f,0x84,0x00,0x00,0x00,0x00,0x00
.Lsqrx8x_zero:
	.byte	0x3e
	movdqa	%xmm0,0*8($tptr)
	movdqa	%xmm0,2*8($tptr)
	movdqa	%xmm0,4*8($tptr)
	movdqa	%xmm0,6*8($tptr)
.Lsqr8x_zero_start:			# aligned at 32
	movdqa	%xmm0,8*8($tptr)
	movdqa	%xmm0,10*8($tptr)
	movdqa	%xmm0,12*8($tptr)
	movdqa	%xmm0,14*8($tptr)
	lea	16*8($tptr),$tptr
	sub	\$64,$num
	jnz	.Lsqrx8x_zero

	mov	0*8($aptr),%rdx		# a[0], modulo-scheduled
	#xor	%r9,%r9			# t[1], ex-$num, zero already
	xor	%r10,%r10
	xor	%r11,%r11
	xor	%r12,%r12
	xor	%r13,%r13
	xor	%r14,%r14
	xor	%r15,%r15
	lea	48+8(%rsp),$tptr
	xor	$zero,$zero		# cf=0, cf=0
	jmp	.Lsqrx8x_outer_loop

.align	32
.Lsqrx8x_outer_loop:
	mulx	1*8($aptr),%r8,%rax	# a[1]*a[0]
	adcx	%r9,%r8			# a[1]*a[0]+=t[1]
	adox	%rax,%r10
	mulx	2*8($aptr),%r9,%rax	# a[2]*a[0]
	adcx	%r10,%r9
	adox	%rax,%r11
	.byte	0xc4,0xe2,0xab,0xf6,0x86,0x18,0x00,0x00,0x00	# mulx	3*8($aptr),%r10,%rax	# ...
	adcx	%r11,%r10
	adox	%rax,%r12
	.byte	0xc4,0xe2,0xa3,0xf6,0x86,0x20,0x00,0x00,0x00	# mulx	4*8($aptr),%r11,%rax
	adcx	%r12,%r11
	adox	%rax,%r13
	mulx	5*8($aptr),%r12,%rax
	adcx	%r13,%r12
	adox	%rax,%r14
	mulx	6*8($aptr),%r13,%rax
	adcx	%r14,%r13
	adox	%r15,%rax
	mulx	7*8($aptr),%r14,%r15
	 mov	1*8($aptr),%rdx		# a[1]
	adcx	%rax,%r14
	adox	$zero,%r15
	adc	8*8($tptr),%r15
	mov	%r8,1*8($tptr)		# t[1]
	mov	%r9,2*8($tptr)		# t[2]
	sbb	$carry,$carry		# mov %cf,$carry
	xor	$zero,$zero		# cf=0, of=0


	mulx	2*8($aptr),%r8,%rbx	# a[2]*a[1]
	mulx	3*8($aptr),%r9,%rax	# a[3]*a[1]
	adcx	%r10,%r8
	adox	%rbx,%r9
	mulx	4*8($aptr),%r10,%rbx	# ...
	adcx	%r11,%r9
	adox	%rax,%r10
	.byte	0xc4,0xe2,0xa3,0xf6,0x86,0x28,0x00,0x00,0x00	# mulx	5*8($aptr),%r11,%rax
	adcx	%r12,%r10
	adox	%rbx,%r11
	.byte	0xc4,0xe2,0x9b,0xf6,0x9e,0x30,0x00,0x00,0x00	# mulx	6*8($aptr),%r12,%rbx
	adcx	%r13,%r11
	adox	%r14,%r12
	.byte	0xc4,0x62,0x93,0xf6,0xb6,0x38,0x00,0x00,0x00	# mulx	7*8($aptr),%r13,%r14
	 mov	2*8($aptr),%rdx		# a[2]
	adcx	%rax,%r12
	adox	%rbx,%r13
	adcx	%r15,%r13
	adox	$zero,%r14		# of=0
	adcx	$zero,%r14		# cf=0

	mov	%r8,3*8($tptr)		# t[3]
	mov	%r9,4*8($tptr)		# t[4]

	mulx	3*8($aptr),%r8,%rbx	# a[3]*a[2]
	mulx	4*8($aptr),%r9,%rax	# a[4]*a[2]
	adcx	%r10,%r8
	adox	%rbx,%r9
	mulx	5*8($aptr),%r10,%rbx	# ...
	adcx	%r11,%r9
	adox	%rax,%r10
	.byte	0xc4,0xe2,0xa3,0xf6,0x86,0x30,0x00,0x00,0x00	# mulx	6*8($aptr),%r11,%rax
	adcx	%r12,%r10
	adox	%r13,%r11
	.byte	0xc4,0x62,0x9b,0xf6,0xae,0x38,0x00,0x00,0x00	# mulx	7*8($aptr),%r12,%r13
	.byte	0x3e
	 mov	3*8($aptr),%rdx		# a[3]
	adcx	%rbx,%r11
	adox	%rax,%r12
	adcx	%r14,%r12
	mov	%r8,5*8($tptr)		# t[5]
	mov	%r9,6*8($tptr)		# t[6]
	 mulx	4*8($aptr),%r8,%rax	# a[4]*a[3]
	adox	$zero,%r13		# of=0
	adcx	$zero,%r13		# cf=0

	mulx	5*8($aptr),%r9,%rbx	# a[5]*a[3]
	adcx	%r10,%r8
	adox	%rax,%r9
	mulx	6*8($aptr),%r10,%rax	# ...
	adcx	%r11,%r9
	adox	%r12,%r10
	mulx	7*8($aptr),%r11,%r12
	 mov	4*8($aptr),%rdx		# a[4]
	 mov	5*8($aptr),%r14		# a[5]
	adcx	%rbx,%r10
	adox	%rax,%r11
	 mov	6*8($aptr),%r15		# a[6]
	adcx	%r13,%r11
	adox	$zero,%r12		# of=0
	adcx	$zero,%r12		# cf=0

	mov	%r8,7*8($tptr)		# t[7]
	mov	%r9,8*8($tptr)		# t[8]

	mulx	%r14,%r9,%rax		# a[5]*a[4]
	 mov	7*8($aptr),%r8		# a[7]
	adcx	%r10,%r9
	mulx	%r15,%r10,%rbx		# a[6]*a[4]
	adox	%rax,%r10
	adcx	%r11,%r10
	mulx	%r8,%r11,%rax		# a[7]*a[4]
	 mov	%r14,%rdx		# a[5]
	adox	%rbx,%r11
	adcx	%r12,%r11
	#adox	$zero,%rax		# of=0
	adcx	$zero,%rax		# cf=0

	mulx	%r15,%r14,%rbx		# a[6]*a[5]
	mulx	%r8,%r12,%r13		# a[7]*a[5]
	 mov	%r15,%rdx		# a[6]
	 lea	8*8($aptr),$aptr
	adcx	%r14,%r11
	adox	%rbx,%r12
	adcx	%rax,%r12
	adox	$zero,%r13

	.byte	0x67,0x67
	mulx	%r8,%r8,%r14		# a[7]*a[6]
	adcx	%r8,%r13
	adcx	$zero,%r14

	cmp	8+8(%rsp),$aptr
	je	.Lsqrx8x_outer_break

	neg	$carry			# mov $carry,%cf
	mov	\$-8,%rcx
	mov	$zero,%r15
	mov	8*8($tptr),%r8
	adcx	9*8($tptr),%r9		# +=t[9]
	adcx	10*8($tptr),%r10	# ...
	adcx	11*8($tptr),%r11
	adc	12*8($tptr),%r12
	adc	13*8($tptr),%r13
	adc	14*8($tptr),%r14
	adc	15*8($tptr),%r15
	lea	($aptr),$aaptr
	lea	2*64($tptr),$tptr
	sbb	%rax,%rax		# mov %cf,$carry

	mov	-64($aptr),%rdx		# a[0]
	mov	%rax,16+8(%rsp)		# offload $carry
	mov	$tptr,24+8(%rsp)

	#lea	8*8($tptr),$tptr	# see 2*8*8($tptr) above
	xor	%eax,%eax		# cf=0, of=0
	jmp	.Lsqrx8x_loop

.align	32
.Lsqrx8x_loop:
	mov	%r8,%rbx
	mulx	0*8($aaptr),%rax,%r8	# a[8]*a[i]
	adcx	%rax,%rbx		# +=t[8]
	adox	%r9,%r8

	mulx	1*8($aaptr),%rax,%r9	# ...
	adcx	%rax,%r8
	adox	%r10,%r9

	mulx	2*8($aaptr),%rax,%r10
	adcx	%rax,%r9
	adox	%r11,%r10

	mulx	3*8($aaptr),%rax,%r11
	adcx	%rax,%r10
	adox	%r12,%r11

	.byte	0xc4,0x62,0xfb,0xf6,0xa5,0x20,0x00,0x00,0x00	# mulx	4*8($aaptr),%rax,%r12
	adcx	%rax,%r11
	adox	%r13,%r12

	mulx	5*8($aaptr),%rax,%r13
	adcx	%rax,%r12
	adox	%r14,%r13

	mulx	6*8($aaptr),%rax,%r14
	 mov	%rbx,($tptr,%rcx,8)	# store t[8+i]
	 mov	\$0,%ebx
	adcx	%rax,%r13
	adox	%r15,%r14

	.byte	0xc4,0x62,0xfb,0xf6,0xbd,0x38,0x00,0x00,0x00	# mulx	7*8($aaptr),%rax,%r15
	 mov	8($aptr,%rcx,8),%rdx	# a[i]
	adcx	%rax,%r14
	adox	%rbx,%r15		# %rbx is 0, of=0
	adcx	%rbx,%r15		# cf=0

	.byte	0x67
	inc	%rcx			# of=0
	jnz	.Lsqrx8x_loop

	lea	8*8($aaptr),$aaptr
	mov	\$-8,%rcx
	cmp	8+8(%rsp),$aaptr	# done?
	je	.Lsqrx8x_break

	sub	16+8(%rsp),%rbx		# mov 16(%rsp),%cf
	.byte	0x66
	mov	-64($aptr),%rdx
	adcx	0*8($tptr),%r8
	adcx	1*8($tptr),%r9
	adc	2*8($tptr),%r10
	adc	3*8($tptr),%r11
	adc	4*8($tptr),%r12
	adc	5*8($tptr),%r13
	adc	6*8($tptr),%r14
	adc	7*8($tptr),%r15
	lea	8*8($tptr),$tptr
	.byte	0x67
	sbb	%rax,%rax		# mov %cf,%rax
	xor	%ebx,%ebx		# cf=0, of=0
	mov	%rax,16+8(%rsp)		# offload carry
	jmp	.Lsqrx8x_loop

.align	32
.Lsqrx8x_break:
	xor	$zero,$zero
	sub	16+8(%rsp),%rbx		# mov 16(%rsp),%cf
	adcx	$zero,%r8
	mov	24+8(%rsp),$carry	# initial $tptr, borrow $carry
	adcx	$zero,%r9
	mov	0*8($aptr),%rdx		# a[8], modulo-scheduled
	adc	\$0,%r10
	mov	%r8,0*8($tptr)
	adc	\$0,%r11
	adc	\$0,%r12
	adc	\$0,%r13
	adc	\$0,%r14
	adc	\$0,%r15
	cmp	$carry,$tptr		# cf=0, of=0
	je	.Lsqrx8x_outer_loop

	mov	%r9,1*8($tptr)
	 mov	1*8($carry),%r9
	mov	%r10,2*8($tptr)
	 mov	2*8($carry),%r10
	mov	%r11,3*8($tptr)
	 mov	3*8($carry),%r11
	mov	%r12,4*8($tptr)
	 mov	4*8($carry),%r12
	mov	%r13,5*8($tptr)
	 mov	5*8($carry),%r13
	mov	%r14,6*8($tptr)
	 mov	6*8($carry),%r14
	mov	%r15,7*8($tptr)
	 mov	7*8($carry),%r15
	mov	$carry,$tptr
	jmp	.Lsqrx8x_outer_loop

.align	32
.Lsqrx8x_outer_break:
	mov	%r9,9*8($tptr)		# t[9]
	 movq	%xmm3,%rcx		# -$num
	mov	%r10,10*8($tptr)	# ...
	mov	%r11,11*8($tptr)
	mov	%r12,12*8($tptr)
	mov	%r13,13*8($tptr)
	mov	%r14,14*8($tptr)
___
}{
my $i="%rcx";
$code.=<<___;
	lea	48+8(%rsp),$tptr
	mov	($aptr,$i),%rdx		# a[0]

	mov	8($tptr),$A0[1]		# t[1]
	xor	$A0[0],$A0[0]		# t[0], of=0, cf=0
	mov	0+8(%rsp),$num		# restore $num
	adox	$A0[1],$A0[1]
	 mov	16($tptr),$A1[0]	# t[2]	# prefetch
	 mov	24($tptr),$A1[1]	# t[3]	# prefetch
	#jmp	.Lsqrx4x_shift_n_add	# happens to be aligned

.align	32
.Lsqrx4x_shift_n_add:
	mulx	%rdx,%rax,%rbx
	 adox	$A1[0],$A1[0]
	adcx	$A0[0],%rax
	 .byte	0x48,0x8b,0x94,0x0e,0x08,0x00,0x00,0x00	# mov	8($aptr,$i),%rdx	# a[i+1]	# prefetch
	 .byte	0x4c,0x8b,0x97,0x20,0x00,0x00,0x00	# mov	32($tptr),$A0[0]	# t[2*i+4]	# prefetch
	 adox	$A1[1],$A1[1]
	adcx	$A0[1],%rbx
	 mov	40($tptr),$A0[1]		# t[2*i+4+1]	# prefetch
	mov	%rax,0($tptr)
	mov	%rbx,8($tptr)

	mulx	%rdx,%rax,%rbx
	 adox	$A0[0],$A0[0]
	adcx	$A1[0],%rax
	 mov	16($aptr,$i),%rdx	# a[i+2]	# prefetch
	 mov	48($tptr),$A1[0]	# t[2*i+6]	# prefetch
	 adox	$A0[1],$A0[1]
	adcx	$A1[1],%rbx
	 mov	56($tptr),$A1[1]	# t[2*i+6+1]	# prefetch
	mov	%rax,16($tptr)
	mov	%rbx,24($tptr)

	mulx	%rdx,%rax,%rbx
	 adox	$A1[0],$A1[0]
	adcx	$A0[0],%rax
	 mov	24($aptr,$i),%rdx	# a[i+3]	# prefetch
	 lea	32($i),$i
	 mov	64($tptr),$A0[0]	# t[2*i+8]	# prefetch
	 adox	$A1[1],$A1[1]
	adcx	$A0[1],%rbx
	 mov	72($tptr),$A0[1]	# t[2*i+8+1]	# prefetch
	mov	%rax,32($tptr)
	mov	%rbx,40($tptr)

	mulx	%rdx,%rax,%rbx
	 adox	$A0[0],$A0[0]
	adcx	$A1[0],%rax
	jrcxz	.Lsqrx4x_shift_n_add_break
	 .byte	0x48,0x8b,0x94,0x0e,0x00,0x00,0x00,0x00	# mov	0($aptr,$i),%rdx	# a[i+4]	# prefetch
	 adox	$A0[1],$A0[1]
	adcx	$A1[1],%rbx
	 mov	80($tptr),$A1[0]	# t[2*i+10]	# prefetch
	 mov	88($tptr),$A1[1]	# t[2*i+10+1]	# prefetch
	mov	%rax,48($tptr)
	mov	%rbx,56($tptr)
	lea	64($tptr),$tptr
	nop
	jmp	.Lsqrx4x_shift_n_add

.align	32
.Lsqrx4x_shift_n_add_break:
	adcx	$A1[1],%rbx
	mov	%rax,48($tptr)
	mov	%rbx,56($tptr)
	lea	64($tptr),$tptr		# end of t[] buffer
___
}
######################################################################
# Montgomery reduction part, "word-by-word" algorithm.
#
# This new path is inspired by multiple submissions from Intel, by
# Shay Gueron, Vlad Krasnov, Erdinc Ozturk, James Guilford,
# Vinodh Gopal...
{
my ($nptr,$carry,$m0)=("%rbp","%rsi","%rdx");

$code.=<<___;
	movq	%xmm2,$nptr
__bn_sqrx8x_reduction:
	xor	%eax,%eax		# initial top-most carry bit
	mov	32+8(%rsp),%rbx		# n0
	mov	48+8(%rsp),%rdx		# "%r8", 8*0($tptr)
	lea	-8*8($nptr,$num),%rcx	# end of n[]
	#lea	48+8(%rsp,$num,2),$tptr	# end of t[] buffer
	mov	%rcx, 0+8(%rsp)		# save end of n[]
	mov	$tptr,8+8(%rsp)		# save end of t[]

	lea	48+8(%rsp),$tptr		# initial t[] window
	jmp	.Lsqrx8x_reduction_loop

.align	32
.Lsqrx8x_reduction_loop:
	mov	8*1($tptr),%r9
	mov	8*2($tptr),%r10
	mov	8*3($tptr),%r11
	mov	8*4($tptr),%r12
	mov	%rdx,%r8
	imulq	%rbx,%rdx		# n0*a[i]
	mov	8*5($tptr),%r13
	mov	8*6($tptr),%r14
	mov	8*7($tptr),%r15
	mov	%rax,24+8(%rsp)		# store top-most carry bit

	lea	8*8($tptr),$tptr
	xor	$carry,$carry		# cf=0,of=0
	mov	\$-8,%rcx
	jmp	.Lsqrx8x_reduce

.align	32
.Lsqrx8x_reduce:
	mov	%r8, %rbx
	mulx	8*0($nptr),%rax,%r8	# n[0]
	adcx	%rbx,%rax		# discarded
	adox	%r9,%r8

	mulx	8*1($nptr),%rbx,%r9	# n[1]
	adcx	%rbx,%r8
	adox	%r10,%r9

	mulx	8*2($nptr),%rbx,%r10
	adcx	%rbx,%r9
	adox	%r11,%r10

	mulx	8*3($nptr),%rbx,%r11
	adcx	%rbx,%r10
	adox	%r12,%r11

	.byte	0xc4,0x62,0xe3,0xf6,0xa5,0x20,0x00,0x00,0x00	# mulx	8*4($nptr),%rbx,%r12
	 mov	%rdx,%rax
	 mov	%r8,%rdx
	adcx	%rbx,%r11
	adox	%r13,%r12

	 mulx	32+8(%rsp),%rbx,%rdx	# %rdx discarded
	 mov	%rax,%rdx
	 mov	%rax,64+48+8(%rsp,%rcx,8)	# put aside n0*a[i]

	mulx	8*5($nptr),%rax,%r13
	adcx	%rax,%r12
	adox	%r14,%r13

	mulx	8*6($nptr),%rax,%r14
	adcx	%rax,%r13
	adox	%r15,%r14

	mulx	8*7($nptr),%rax,%r15
	 mov	%rbx,%rdx
	adcx	%rax,%r14
	adox	$carry,%r15		# $carry is 0
	adcx	$carry,%r15		# cf=0

	.byte	0x67,0x67,0x67
	inc	%rcx			# of=0
	jnz	.Lsqrx8x_reduce

	mov	$carry,%rax		# xor	%rax,%rax
	cmp	0+8(%rsp),$nptr		# end of n[]?
	jae	.Lsqrx8x_no_tail

	mov	48+8(%rsp),%rdx		# pull n0*a[0]
	add	8*0($tptr),%r8
	lea	8*8($nptr),$nptr
	mov	\$-8,%rcx
	adcx	8*1($tptr),%r9
	adcx	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	lea	8*8($tptr),$tptr
	sbb	%rax,%rax		# top carry

	xor	$carry,$carry		# of=0, cf=0
	mov	%rax,16+8(%rsp)
	jmp	.Lsqrx8x_tail

.align	32
.Lsqrx8x_tail:
	mov	%r8,%rbx
	mulx	8*0($nptr),%rax,%r8
	adcx	%rax,%rbx
	adox	%r9,%r8

	mulx	8*1($nptr),%rax,%r9
	adcx	%rax,%r8
	adox	%r10,%r9

	mulx	8*2($nptr),%rax,%r10
	adcx	%rax,%r9
	adox	%r11,%r10

	mulx	8*3($nptr),%rax,%r11
	adcx	%rax,%r10
	adox	%r12,%r11

	.byte	0xc4,0x62,0xfb,0xf6,0xa5,0x20,0x00,0x00,0x00	# mulx	8*4($nptr),%rax,%r12
	adcx	%rax,%r11
	adox	%r13,%r12

	mulx	8*5($nptr),%rax,%r13
	adcx	%rax,%r12
	adox	%r14,%r13

	mulx	8*6($nptr),%rax,%r14
	adcx	%rax,%r13
	adox	%r15,%r14

	mulx	8*7($nptr),%rax,%r15
	 mov	72+48+8(%rsp,%rcx,8),%rdx	# pull n0*a[i]
	adcx	%rax,%r14
	adox	$carry,%r15
	 mov	%rbx,($tptr,%rcx,8)	# save result
	 mov	%r8,%rbx
	adcx	$carry,%r15		# cf=0

	inc	%rcx			# of=0
	jnz	.Lsqrx8x_tail

	cmp	0+8(%rsp),$nptr		# end of n[]?
	jae	.Lsqrx8x_tail_done	# break out of loop

	sub	16+8(%rsp),$carry	# mov 16(%rsp),%cf
	 mov	48+8(%rsp),%rdx		# pull n0*a[0]
	 lea	8*8($nptr),$nptr
	adc	8*0($tptr),%r8
	adc	8*1($tptr),%r9
	adc	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	lea	8*8($tptr),$tptr
	sbb	%rax,%rax
	sub	\$8,%rcx		# mov	\$-8,%rcx

	xor	$carry,$carry		# of=0, cf=0
	mov	%rax,16+8(%rsp)
	jmp	.Lsqrx8x_tail

.align	32
.Lsqrx8x_tail_done:
	xor	%rax,%rax
	add	24+8(%rsp),%r8		# can this overflow?
	adc	\$0,%r9
	adc	\$0,%r10
	adc	\$0,%r11
	adc	\$0,%r12
	adc	\$0,%r13
	adc	\$0,%r14
	adc	\$0,%r15
	adc	\$0,%rax

	sub	16+8(%rsp),$carry	# mov 16(%rsp),%cf
.Lsqrx8x_no_tail:			# %cf is 0 if jumped here
	adc	8*0($tptr),%r8
	 movq	%xmm3,%rcx
	adc	8*1($tptr),%r9
	 mov	8*7($nptr),$carry
	 movq	%xmm2,$nptr		# restore $nptr
	adc	8*2($tptr),%r10
	adc	8*3($tptr),%r11
	adc	8*4($tptr),%r12
	adc	8*5($tptr),%r13
	adc	8*6($tptr),%r14
	adc	8*7($tptr),%r15
	adc	\$0,%rax		# top-most carry

	mov	32+8(%rsp),%rbx		# n0
	mov	8*8($tptr,%rcx),%rdx	# modulo-scheduled "%r8"

	mov	%r8,8*0($tptr)		# store top 512 bits
	 lea	8*8($tptr),%r8		# borrow %r8
	mov	%r9,8*1($tptr)
	mov	%r10,8*2($tptr)
	mov	%r11,8*3($tptr)
	mov	%r12,8*4($tptr)
	mov	%r13,8*5($tptr)
	mov	%r14,8*6($tptr)
	mov	%r15,8*7($tptr)

	lea	8*8($tptr,%rcx),$tptr	# start of current t[] window
	cmp	8+8(%rsp),%r8		# end of t[]?
	jb	.Lsqrx8x_reduction_loop
	ret
.cfi_endproc
.size	bn_sqrx8x_internal,.-bn_sqrx8x_internal
___
}
##############################################################
# Post-condition, 4x unrolled
#
{
my ($rptr,$nptr)=("%rdx","%rbp");
$code.=<<___;
.align	32
.type	__bn_postx4x_internal,\@abi-omnipotent
__bn_postx4x_internal:
.cfi_startproc
	mov	8*0($nptr),%r12
	mov	%rcx,%r10		# -$num
	mov	%rcx,%r9		# -$num
	neg	%rax
	sar	\$3+2,%rcx
	#lea	48+8(%rsp,%r9),$tptr
	movq	%xmm1,$rptr		# restore $rptr
	movq	%xmm1,$aptr		# prepare for back-to-back call
	dec	%r12			# so that after 'not' we get -n[0]
	mov	8*1($nptr),%r13
	xor	%r8,%r8
	mov	8*2($nptr),%r14
	mov	8*3($nptr),%r15
	jmp	.Lsqrx4x_sub_entry

.align	16
.Lsqrx4x_sub:
	mov	8*0($nptr),%r12
	mov	8*1($nptr),%r13
	mov	8*2($nptr),%r14
	mov	8*3($nptr),%r15
.Lsqrx4x_sub_entry:
	andn	%rax,%r12,%r12
	lea	8*4($nptr),$nptr
	andn	%rax,%r13,%r13
	andn	%rax,%r14,%r14
	andn	%rax,%r15,%r15

	neg	%r8			# mov %r8,%cf
	adc	8*0($tptr),%r12
	adc	8*1($tptr),%r13
	adc	8*2($tptr),%r14
	adc	8*3($tptr),%r15
	mov	%r12,8*0($rptr)
	lea	8*4($tptr),$tptr
	mov	%r13,8*1($rptr)
	sbb	%r8,%r8			# mov %cf,%r8
	mov	%r14,8*2($rptr)
	mov	%r15,8*3($rptr)
	lea	8*4($rptr),$rptr

	inc	%rcx
	jnz	.Lsqrx4x_sub

	neg	%r9			# restore $num

	ret
.cfi_endproc
.size	__bn_postx4x_internal,.-__bn_postx4x_internal
___
}
}}}
{
my ($inp,$num,$tbl,$idx)=$win64?("%rcx","%edx","%r8", "%r9d") : # Win64 order
				("%rdi","%esi","%rdx","%ecx");  # Unix order
my $out=$inp;
my $STRIDE=2**5*8;
my $N=$STRIDE/4;

$code.=<<___;
.globl	bn_scatter5
.type	bn_scatter5,\@abi-omnipotent
.align	16
bn_scatter5:
.cfi_startproc
	_CET_ENDBR
	cmp	\$0, $num
	jz	.Lscatter_epilogue

	# $tbl stores 32 entries, t0 through t31. Each entry has $num words.
	# They are interleaved in memory as follows:
	#
	#  t0[0]      t1[0]      t2[0]      ... t31[0]
	#  t0[1]      t1[1]      t2[1]      ... t31[1]
	#  ...
	#  t0[$num-1] t1[$num-1] t2[$num-1] ... t31[$num-1]

	lea	($tbl,$idx,8),$tbl
.Lscatter:
	mov	($inp),%rax
	lea	8($inp),$inp
	mov	%rax,($tbl)
	lea	32*8($tbl),$tbl
	sub	\$1,$num
	jnz	.Lscatter
.Lscatter_epilogue:
	ret
.cfi_endproc
.size	bn_scatter5,.-bn_scatter5

.globl	bn_gather5
.type	bn_gather5,\@abi-omnipotent
.align	32
bn_gather5:
.cfi_startproc
.LSEH_begin_bn_gather5:			# Win64 thing, but harmless in other cases
	_CET_ENDBR
	# I can't trust assembler to use specific encoding:-(
	.byte	0x4c,0x8d,0x14,0x24			#lea    ( 
.cfi_def_cfa_register	
	.byte	0x48,0x81,0xec,0x08,0x01,0x00,0x00	#sub	$0x108, 
	lea	.Linc( 
	and	\$-16, 

	movd	$idx, 
	movdqa	0( 
	movdqa	16( 
	lea	128($tbl), 
	lea	128( 

	pshufd	\$0, 
	movdqa	
	movdqa	
___
########################################################################
# Calculate masks by comparing 0..31 to $idx and save result to stack.
#
# We compute sixteen 16-byte masks and store them on the stack. Mask i is stored
# in `16*i - 128`( 
# idx == 2*i + 1 in its lower and upper halves, respectively. Mask calculations
# are scheduled in groups of four.
for($i=0;$i<$STRIDE/16;$i+=4) {
$code.=<<___;
	paddd	
	pcmpeqd	
___
$code.=<<___	if ($i);
	movdqa	
___
$code.=<<___;
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	

	paddd	
	pcmpeqd	
	movdqa	
	movdqa	
___
}
$code.=<<___;
	movdqa	
	jmp	.Lgather

.align	32
.Lgather:
	pxor	
	pxor	
___
for($i=0;$i<$STRIDE/16;$i+=4) {
# Combine the masks with the corresponding table entries to select the correct
# entry.
$code.=<<___;
	movdqa	`16*($i+0)-128`( 
	movdqa	`16*($i+1)-128`( 
	movdqa	`16*($i+2)-128`( 
	pand	`16*($i+0)-128`( 
	movdqa	`16*($i+3)-128`( 
	pand	`16*($i+1)-128`( 
	por	
	pand	`16*($i+2)-128`( 
	por	
	pand	`16*($i+3)-128`( 
	por	
	por	
___
}
$code.=<<___;
	por	
	lea	$STRIDE( 
	# Combine the upper and lower halves of 
	pshufd	\$0x4e, 
	por	
	movq	
	lea	8($out),$out
	sub	\$1,$num
	jnz	.Lgather

	lea	( 
.cfi_def_cfa_register	
	ret
.LSEH_end_bn_gather5:
.cfi_endproc
.size	bn_gather5,.-bn_gather5
___
}
$code.=<<___;
.section .rodata
.align	64
.Linc:
	.long	0,0, 1,1
	.long	2,2, 2,2
.asciz	"Montgomery Multiplication with scatter/gather for x86_64, CRYPTOGAMS by <appro\@openssl.org>"
.text
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

	mov	4( 
	lea	( 
	cmp	
	jb	.Lcommon_pop_regs

	mov	152($context), 

	mov	8( 
	lea	( 
	cmp	
	jae	.Lcommon_seh_tail

	lea	.Lmul4x_epilogue( 
	cmp	
	ja	.Lbody_40

	mov	192($context), 
	mov	8( 

	jmp	.Lcommon_pop_regs

.Lbody_40:
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
.size	mul_handler,.-mul_handler

.section	.pdata
.align	4
	.rva	.LSEH_begin_bn_mul4x_mont_gather5
	.rva	.LSEH_end_bn_mul4x_mont_gather5
	.rva	.LSEH_info_bn_mul4x_mont_gather5

	.rva	.LSEH_begin_bn_power5_nohw
	.rva	.LSEH_end_bn_power5_nohw
	.rva	.LSEH_info_bn_power5_nohw
___
$code.=<<___ if ($addx);
	.rva	.LSEH_begin_bn_mulx4x_mont_gather5
	.rva	.LSEH_end_bn_mulx4x_mont_gather5
	.rva	.LSEH_info_bn_mulx4x_mont_gather5

	.rva	.LSEH_begin_bn_powerx5
	.rva	.LSEH_end_bn_powerx5
	.rva	.LSEH_info_bn_powerx5
___
$code.=<<___;
	.rva	.LSEH_begin_bn_gather5
	.rva	.LSEH_end_bn_gather5
	.rva	.LSEH_info_bn_gather5

.section	.xdata
.align	8
.LSEH_info_bn_mul4x_mont_gather5:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lmul4x_prologue,.Lmul4x_body,.Lmul4x_epilogue		# HandlerData[]
.align	8
.LSEH_info_bn_power5_nohw:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lpower5_prologue,.Lpower5_body,.Lpower5_epilogue	# HandlerData[]
___
$code.=<<___ if ($addx);
.align	8
.LSEH_info_bn_mulx4x_mont_gather5:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lmulx4x_prologue,.Lmulx4x_body,.Lmulx4x_epilogue	# HandlerData[]
.align	8
.LSEH_info_bn_powerx5:
	.byte	9,0,0,0
	.rva	mul_handler
	.rva	.Lpowerx5_prologue,.Lpowerx5_body,.Lpowerx5_epilogue	# HandlerData[]
___
$code.=<<___;
.align	8
.LSEH_info_bn_gather5:
	.byte	0x01,0x0b,0x03,0x0a
	.byte	0x0b,0x01,0x21,0x00	# sub	rsp,0x108
	.byte	0x04,0xa3,0x00,0x00	# lea	r10,(rsp)
.align	8
___
}

$code =~ s/\`([^\`]*)\`/eval($1)/gem;

print $code;
close STDOUT or die "error closing STDOUT: $!";
