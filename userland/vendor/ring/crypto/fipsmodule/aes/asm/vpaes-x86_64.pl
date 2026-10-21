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


######################################################################
## Constant-time SSSE3 AES core implementation.
## version 0.1
##
## By Mike Hamburg (Stanford University), 2009
## Public domain.
##
## For details see http://shiftleft.org/papers/vector_aes/ and
## http://crypto.stanford.edu/vpaes/.

######################################################################
# September 2011.
#
# Interface to OpenSSL as "almost" drop-in replacement for
# aes-x86_64.pl. "Almost" refers to the fact that AES_cbc_encrypt
# doesn't handle partial vectors (doesn't have to if called from
# EVP only). "Drop-in" implies that this module doesn't share key
# schedule structure with the original nor does it make assumption
# about its alignment...
#
# Performance summary. aes-x86_64.pl column lists large-block CBC
# encrypt/decrypt/with-hyper-threading-off(*) results in cycles per
# byte processed with 128-bit key, and vpaes-x86_64.pl column -
# [also large-block CBC] encrypt/decrypt.
#
#		aes-x86_64.pl		vpaes-x86_64.pl
#
# Core 2(**)	29.6/41.1/14.3		21.9/25.2(***)
# Nehalem	29.6/40.3/14.6		10.0/11.8
# Atom		57.3/74.2/32.1		60.9/77.2(***)
# Silvermont	52.7/64.0/19.5		48.8/60.8(***)
# Goldmont	38.9/49.0/17.8		10.6/12.6
#
# (*)	"Hyper-threading" in the context refers rather to cache shared
#	among multiple cores, than to specifically Intel HTT. As vast
#	majority of contemporary cores share cache, slower code path
#	is common place. In other words "with-hyper-threading-off"
#	results are presented mostly for reference purposes.
#
# (**)	"Core 2" refers to initial 65nm design, a.k.a. Conroe.
#
# (***)	Less impressive improvement on Core 2 and Atom is due to slow
#	pshufb,	yet it's respectable +36 
#	(as implied, over "hyper-threading-safe" code path).
#
#						<appro@openssl.org>

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

$PREFIX="vpaes";

$code.=<<___;
.text

##
##  _aes_encrypt_core
##
##  AES-encrypt 
##
##  Inputs:
##     
##     
##    ( 
##
##  Output in 
##  Clobbers  
##  Preserves 
##
##
.type	_vpaes_encrypt_core,\@abi-omnipotent
.align 16
_vpaes_encrypt_core:
.cfi_startproc
	mov	
	mov	\$16,	
	mov	240( 
	movdqa	
	movdqa	.Lk_ipt( 
	pandn	
	movdqu	( 
	psrld	\$4,	
	pand	
	pshufb	
	movdqa	.Lk_ipt+16( 
	pshufb	
	pxor	
	add	\$16,	
	pxor	
	lea	.Lk_mc_backward( 
	jmp	.Lenc_entry

.align 16
.Lenc_loop:
	# middle of middle round
	movdqa  
	movdqa  
	pshufb  
	pshufb  
	pxor	
	movdqa  
	pxor	
	movdqa	-0x40( 
	pshufb	
	movdqa	( 
	movdqa	
	pshufb	
	movdqa	
	pxor	
	pshufb  
	add	\$16,	
	pxor	
	pshufb	
	add	\$16,	
	pxor	
	pshufb  
	and	\$0x30,	
	sub	\$1, 
	pxor	

.Lenc_entry:
	# top of round
	movdqa  
	movdqa	
	pandn	
	psrld	\$4,   	
	pand	
	pshufb  
	movdqa	
	pxor	
	pshufb  
	movdqa	
	pxor	
	pshufb	
	movdqa	
	pxor	
	pshufb  
	movdqa	
	pxor	
	pshufb  
	movdqu	( 
	pxor	
	jnz	.Lenc_loop

	# middle of last round
	movdqa	-0x60( 
	movdqa	-0x50( 
	pshufb  
	pxor	
	pshufb  
	movdqa	0x40( 
	pxor	
	pshufb	
	ret
.cfi_endproc
.size	_vpaes_encrypt_core,.-_vpaes_encrypt_core

##
##  _aes_encrypt_core_2x
##
##  AES-encrypt 
##
##  Inputs:
##     
##     
##    ( 
##
##  Output in 
##  Clobbers  
##  Preserves 
##
##  This function stitches two parallel instances of _vpaes_encrypt_core. x86_64
##  provides 16 XMM registers. _vpaes_encrypt_core computes over six registers
##  ( 
##  from _vpaes_preheat ( 
##  so we spill some of 
##  
##  remainder are read once per round and are spilled to memory. This leaves two
##  registers preserved for the caller.
##
##  Thus, of the two _vpaes_encrypt_core instances, the first uses ( 
##  as before. The second uses 
##  below. Add 8 to 
##  indented by one space.
##
##
.type	_vpaes_encrypt_core_2x,\@abi-omnipotent
.align 16
_vpaes_encrypt_core_2x:
.cfi_startproc
	mov	
	mov	\$16,	
	mov	240( 
	movdqa	
	 movdqa	
	movdqa	.Lk_ipt( 
	 movdqa	
	pandn	
	 pandn	
	movdqu	( 
	 # Also use 
	psrld	\$4,	
	 psrld	\$4,	
	pand	
	 pand	
	pshufb	
	 pshufb	
	movdqa	.Lk_ipt+16( 
	 movdqa	
	pshufb	
	 pshufb	
	pxor	
	 pxor	
	add	\$16,	
	pxor	
	 pxor	
	lea	.Lk_mc_backward( 
	jmp	.Lenc2x_entry

.align 16
.Lenc2x_loop:
	# middle of middle round
	movdqa  .Lk_sb1( 
	movdqa  .Lk_sb1+16( 
	 movdqa	
	 movdqa	
	pshufb  
	 pshufb	
	pshufb  
	 pshufb	
	pxor	
	 pxor	
	movdqa  .Lk_sb2( 
	 movdqa	
	pxor	
	 pxor	
	movdqa	-0x40( 
	 # Also use 
	pshufb	
	 pshufb	
	movdqa	( 
	 # Also use 
	movdqa	.Lk_sb2+16( 
	 movdqa	
	pshufb	
	 pshufb	
	movdqa	
	 movdqa	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	add	\$16,	
	pxor	
	 pxor	
	pshufb	
	 pshufb	
	add	\$16,	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	and	\$0x30,	
	sub	\$1, 
	pxor	
	 pxor	

.Lenc2x_entry:
	# top of round
	movdqa  
	 movdqa	
	movdqa	.Lk_inv+16( 
	 movdqa	
	pandn	
	 pandn	
	psrld	\$4,   	
	 psrld	\$4,	
	pand	
	 pand	
	pshufb  
	 pshufb	
	movdqa	
	 movdqa	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	movdqa	
	 movdqa	
	pxor	
	 pxor	
	pshufb	
	 pshufb	
	movdqa	
	 movdqa	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	movdqa	
	 movdqa	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	movdqu	( 
	 # Also use 
	pxor	
	 pxor	
	jnz	.Lenc2x_loop

	# middle of last round
	movdqa	-0x60( 
	movdqa	-0x50( 
	 movdqa	
	 movdqa	
	pshufb  
	 pshufb	
	pxor	
	 pxor	
	pshufb  
	 pshufb	
	movdqa	0x40( 
	 # Also use 
	pxor	
	 pxor	
	pshufb	
	 pshufb	
	ret
.cfi_endproc
.size	_vpaes_encrypt_core_2x,.-_vpaes_encrypt_core_2x

########################################################
##                                                    ##
##                  AES key schedule                  ##
##                                                    ##
########################################################
.type	_vpaes_schedule_core,\@abi-omnipotent
.align	16
_vpaes_schedule_core:
.cfi_startproc
	# rdi = key
	# rsi = size in bits
	# rdx = buffer
	# rcx = direction.  0=encrypt, 1=decrypt

	call	_vpaes_preheat		# load the tables
	movdqa	.Lk_rcon( 
	movdqu	( 

	# input transform
	movdqa	
	lea	.Lk_ipt( 
	call	_vpaes_schedule_transform
	movdqa	

	lea	.Lk_sr( 

	# encrypting, output zeroth round key after transform
	movdqu	

.Lschedule_go:
	cmp	\$192,	
	ja	.Lschedule_256
	# 192-bit key support was removed.
	# 128: fall though

##
##  .schedule_128
##
##  128-bit specific part of key schedule.
##
##  This schedule is really simple, because all its parts
##  are accomplished by the subroutines.
##
.Lschedule_128:
	mov	\$10, 

.Loop_schedule_128:
	call 	_vpaes_schedule_round
	dec	
	jz 	.Lschedule_mangle_last
	call	_vpaes_schedule_mangle	# write output
	jmp 	.Loop_schedule_128

##
##  .aes_schedule_256
##
##  256-bit specific part of key schedule.
##
##  The structure here is very similar to the 128-bit
##  schedule, but with an additional "low side" in
##  
##  high side's, except no rcon and no rotation.
##
.align	16
.Lschedule_256:
	movdqu	16(%rdi),%xmm0		# load key part 2 (unaligned)
	call	_vpaes_schedule_transform	# input transform
	mov	\$7, %esi

.Loop_schedule_256:
	call	_vpaes_schedule_mangle	# output low result
	movdqa	%xmm0,	%xmm6		# save cur_lo in xmm6

	# high round
	call	_vpaes_schedule_round
	dec	%rsi
	jz 	.Lschedule_mangle_last
	call	_vpaes_schedule_mangle

	# low round. swap xmm7 and xmm6
	pshufd	\$0xFF,	%xmm0,	%xmm0
	movdqa	%xmm7,	%xmm5
	movdqa	%xmm6,	%xmm7
	call	_vpaes_schedule_low_round
	movdqa	%xmm5,	%xmm7

	jmp	.Loop_schedule_256


##
##  .aes_schedule_mangle_last
##
##  Mangler for last round of key schedule
##  Mangles %xmm0
##    when encrypting, outputs out(%xmm0) ^ 63
##    when decrypting, outputs unskew(%xmm0)
##
##  Always called right before return... jumps to cleanup and exits
##
.align	16
.Lschedule_mangle_last:
	# schedule last round key from xmm0
	lea	.Lk_deskew(%rip),%r11	# prepare to deskew

	# encrypting
	movdqa	(%r8,%r10),%xmm1
	pshufb	%xmm1,	%xmm0		# output permute
	lea	.Lk_opt(%rip),	%r11	# prepare to output transform
	add	\$32,	%rdx

.Lschedule_mangle_last_dec:
	add	\$-16,	%rdx
	pxor	.Lk_s63(%rip),	%xmm0
	call	_vpaes_schedule_transform # output transform
	movdqu	%xmm0,	(%rdx)		# save last key

	# cleanup
	pxor	%xmm0,  %xmm0
	pxor	%xmm1,  %xmm1
	pxor	%xmm2,  %xmm2
	pxor	%xmm3,  %xmm3
	pxor	%xmm4,  %xmm4
	pxor	%xmm5,  %xmm5
	pxor	%xmm6,  %xmm6
	pxor	%xmm7,  %xmm7
	ret
.cfi_endproc
.size	_vpaes_schedule_core,.-_vpaes_schedule_core

##
##  .aes_schedule_round
##
##  Runs one main round of the key schedule on %xmm0, %xmm7
##
##  Specifically, runs subbytes on the high dword of %xmm0
##  then rotates it by one byte and xors into the low dword of
##  %xmm7.
##
##  Adds rcon from low byte of %xmm8, then rotates %xmm8 for
##  next rcon.
##
##  Smears the dwords of %xmm7 by xoring the low into the
##  second low, result into third, result into highest.
##
##  Returns results in %xmm7 = %xmm0.
##  Clobbers %xmm1-%xmm4, %r11.
##
.type	_vpaes_schedule_round,\@abi-omnipotent
.align	16
_vpaes_schedule_round:
.cfi_startproc
	# extract rcon from xmm8
	pxor	%xmm1,	%xmm1
	palignr	\$15,	%xmm8,	%xmm1
	palignr	\$15,	%xmm8,	%xmm8
	pxor	%xmm1,	%xmm7

	# rotate
	pshufd	\$0xFF,	%xmm0,	%xmm0
	palignr	\$1,	%xmm0,	%xmm0

	# fall through...

	# low round: same as high round, but no rotation and no rcon.
_vpaes_schedule_low_round:
	# smear xmm7
	movdqa	%xmm7,	%xmm1
	pslldq	\$4,	%xmm7
	pxor	%xmm1,	%xmm7
	movdqa	%xmm7,	%xmm1
	pslldq	\$8,	%xmm7
	pxor	%xmm1,	%xmm7
	pxor	.Lk_s63(%rip), %xmm7

	# subbytes
	movdqa  %xmm9, 	%xmm1
	pandn	%xmm0, 	%xmm1
	psrld	\$4,    %xmm1		# 1 = i
	pand	%xmm9, 	%xmm0		# 0 = k
	movdqa	%xmm11, %xmm2		# 2 : a/k
	pshufb  %xmm0,  %xmm2		# 2 = a/k
	pxor	%xmm1,	%xmm0		# 0 = j
	movdqa	%xmm10,	%xmm3		# 3 : 1/i
	pshufb  %xmm1, 	%xmm3		# 3 = 1/i
	pxor	%xmm2, 	%xmm3		# 3 = iak = 1/i + a/k
	movdqa	%xmm10,	%xmm4		# 4 : 1/j
	pshufb	%xmm0, 	%xmm4		# 4 = 1/j
	pxor	%xmm2, 	%xmm4		# 4 = jak = 1/j + a/k
	movdqa	%xmm10,	%xmm2		# 2 : 1/iak
	pshufb  %xmm3,	%xmm2		# 2 = 1/iak
	pxor	%xmm0, 	%xmm2		# 2 = io
	movdqa	%xmm10, %xmm3		# 3 : 1/jak
	pshufb  %xmm4,  %xmm3		# 3 = 1/jak
	pxor	%xmm1,  %xmm3		# 3 = jo
	movdqa	%xmm13, %xmm4		# 4 : sbou
	pshufb  %xmm2,  %xmm4		# 4 = sbou
	movdqa	%xmm12, %xmm0		# 0 : sbot
	pshufb  %xmm3,	%xmm0		# 0 = sb1t
	pxor	%xmm4, 	%xmm0		# 0 = sbox output

	# add in smeared stuff
	pxor	%xmm7,	%xmm0
	movdqa	%xmm0,	%xmm7
	ret
.cfi_endproc
.size	_vpaes_schedule_round,.-_vpaes_schedule_round

##
##  .aes_schedule_transform
##
##  Linear-transform %xmm0 according to tables at (%r11)
##
##  Requires that %xmm9 = 0x0F0F... as in preheat
##  Output in %xmm0
##  Clobbers %xmm1, %xmm2
##
.type	_vpaes_schedule_transform,\@abi-omnipotent
.align	16
_vpaes_schedule_transform:
.cfi_startproc
	movdqa	%xmm9,	%xmm1
	pandn	%xmm0,	%xmm1
	psrld	\$4,	%xmm1
	pand	%xmm9,	%xmm0
	movdqa	(%r11), %xmm2 	# lo
	pshufb	%xmm0,	%xmm2
	movdqa	16(%r11), %xmm0 # hi
	pshufb	%xmm1,	%xmm0
	pxor	%xmm2,	%xmm0
	ret
.cfi_endproc
.size	_vpaes_schedule_transform,.-_vpaes_schedule_transform

##
##  .aes_schedule_mangle
##
##  Mangle xmm0 from (basis-transformed) standard version
##  to our version.
##
##  On encrypt,
##    xor with 0x63
##    multiply by circulant 0,1,1,1
##    apply shiftrows transform
##
##  On decrypt,
##    xor with 0x63
##    multiply by "inverse mixcolumns" circulant E,B,D,9
##    deskew
##    apply shiftrows transform
##
##
##  Writes out to (%rdx), and increments or decrements it
##  Keeps track of round number mod 4 in %r8
##  Preserves xmm0
##  Clobbers xmm1-xmm5
##
.type	_vpaes_schedule_mangle,\@abi-omnipotent
.align	16
_vpaes_schedule_mangle:
.cfi_startproc
	movdqa	%xmm0,	%xmm4	# save xmm0 for later
	movdqa	.Lk_mc_forward(%rip),%xmm5

	# encrypting
	add	\$16,	%rdx
	pxor	.Lk_s63(%rip),%xmm4
	pshufb	%xmm5,	%xmm4
	movdqa	%xmm4,	%xmm3
	pshufb	%xmm5,	%xmm4
	pxor	%xmm4,	%xmm3
	pshufb	%xmm5,	%xmm4
	pxor	%xmm4,	%xmm3

.Lschedule_mangle_both:
	movdqa	(%r8,%r10),%xmm1
	pshufb	%xmm1,%xmm3
	add	\$-16,	%r8
	and	\$0x30,	%r8
	movdqu	%xmm3,	(%rdx)
	ret
.cfi_endproc
.size	_vpaes_schedule_mangle,.-_vpaes_schedule_mangle

#
# Interface to OpenSSL
#
.globl	${PREFIX}_set_encrypt_key
.type	${PREFIX}_set_encrypt_key,\@function,3
.align	16
${PREFIX}_set_encrypt_key:
.cfi_startproc
	_CET_ENDBR
#ifdef BORINGSSL_DISPATCH_TEST
.extern        BORINGSSL_function_hit
       movb \$1, BORINGSSL_function_hit+5(%rip)
#endif

___
$code.=<<___ if ($win64);
	lea	-0xb8(%rsp),%rsp
	movaps	%xmm6,0x10(%rsp)
	movaps	%xmm7,0x20(%rsp)
	movaps	%xmm8,0x30(%rsp)
	movaps	%xmm9,0x40(%rsp)
	movaps	%xmm10,0x50(%rsp)
	movaps	%xmm11,0x60(%rsp)
	movaps	%xmm12,0x70(%rsp)
	movaps	%xmm13,0x80(%rsp)
	movaps	%xmm14,0x90(%rsp)
	movaps	%xmm15,0xa0(%rsp)
.Lenc_key_body:
___
$code.=<<___;
	mov	%esi,%eax
	shr	\$5,%eax
	add	\$5,%eax
	mov	%eax,240(%rdx)	# AES_KEY->rounds = nbits/32+5;

	mov	\$0,%ecx
	mov	\$0x30,%r8d
	call	_vpaes_schedule_core
___
$code.=<<___ if ($win64);
	movaps	0x10(%rsp),%xmm6
	movaps	0x20(%rsp),%xmm7
	movaps	0x30(%rsp),%xmm8
	movaps	0x40(%rsp),%xmm9
	movaps	0x50(%rsp),%xmm10
	movaps	0x60(%rsp),%xmm11
	movaps	0x70(%rsp),%xmm12
	movaps	0x80(%rsp),%xmm13
	movaps	0x90(%rsp),%xmm14
	movaps	0xa0(%rsp),%xmm15
	lea	0xb8(%rsp),%rsp
.Lenc_key_epilogue:
___
$code.=<<___;
	xor	%eax,%eax
	ret
.cfi_endproc
.size	${PREFIX}_set_encrypt_key,.-${PREFIX}_set_encrypt_key
___
{
my ($inp,$out,$blocks,$key,$ivp)=("%rdi","%rsi","%rdx","%rcx","%r8");
# void vpaes_ctr32_encrypt_blocks(const uint8_t *inp, uint8_t *out,
#                                 size_t blocks, const AES_KEY *key,
#                                 const uint8_t ivp[16]);
$code.=<<___;
.globl	${PREFIX}_ctr32_encrypt_blocks
.type	${PREFIX}_ctr32_encrypt_blocks,\@function,5
.align	16
${PREFIX}_ctr32_encrypt_blocks:
.cfi_startproc
	_CET_ENDBR
	# _vpaes_encrypt_core and _vpaes_encrypt_core_2x expect the key in %rdx.
	xchg	$key, $blocks
___
($blocks,$key)=($key,$blocks);
$code.=<<___;
	test	$blocks, $blocks
	jz	.Lctr32_abort
___
$code.=<<___ if ($win64);
	lea	-0xb8(%rsp),%rsp
	movaps	%xmm6,0x10(%rsp)
	movaps	%xmm7,0x20(%rsp)
	movaps	%xmm8,0x30(%rsp)
	movaps	%xmm9,0x40(%rsp)
	movaps	%xmm10,0x50(%rsp)
	movaps	%xmm11,0x60(%rsp)
	movaps	%xmm12,0x70(%rsp)
	movaps	%xmm13,0x80(%rsp)
	movaps	%xmm14,0x90(%rsp)
	movaps	%xmm15,0xa0(%rsp)
.Lctr32_body:
___
$code.=<<___;
	movdqu	($ivp), %xmm0		# Load IV.
	movdqa	.Lctr_add_one(%rip), %xmm8
	sub	$inp, $out		# This allows only incrementing $inp.
	call	_vpaes_preheat
	movdqa	%xmm0, %xmm6
	pshufb	.Lrev_ctr(%rip), %xmm6

	test	\$1, $blocks
	jz	.Lctr32_prep_loop

	# Handle one block so the remaining block count is even for
	# _vpaes_encrypt_core_2x.
	movdqu	($inp), %xmm7		# Load input.
	call	_vpaes_encrypt_core
	pxor	%xmm7, %xmm0
	paddd	%xmm8, %xmm6
	movdqu	%xmm0, ($out,$inp)
	sub	\$1, $blocks
	lea	16($inp), $inp
	jz	.Lctr32_done

.Lctr32_prep_loop:
	# _vpaes_encrypt_core_2x leaves only %xmm14 and %xmm15 as spare
	# registers. We maintain two byte-swapped counters in them.
	movdqa	%xmm6, %xmm14
	movdqa	%xmm6, %xmm15
	paddd	%xmm8, %xmm15

.Lctr32_loop:
	movdqa	.Lrev_ctr(%rip), %xmm1	# Set up counters.
	movdqa	%xmm14, %xmm0
	movdqa	%xmm15, %xmm6
	pshufb	%xmm1, %xmm0
	pshufb	%xmm1, %xmm6
	call	_vpaes_encrypt_core_2x
	movdqu	($inp), %xmm1		# Load input.
	movdqu	16($inp), %xmm2
	movdqa	.Lctr_add_two(%rip), %xmm3
	pxor	%xmm1, %xmm0		# XOR input.
	pxor	%xmm2, %xmm6
	paddd	%xmm3, %xmm14		# Increment counters.
	paddd	%xmm3, %xmm15
	movdqu	%xmm0, ($out,$inp)	# Write output.
	movdqu	%xmm6, 16($out,$inp)
	sub	\$2, $blocks		# Advance loop.
	lea	32($inp), $inp
	jnz	.Lctr32_loop

.Lctr32_done:
___
$code.=<<___ if ($win64);
	movaps	0x10(%rsp),%xmm6
	movaps	0x20(%rsp),%xmm7
	movaps	0x30(%rsp),%xmm8
	movaps	0x40(%rsp),%xmm9
	movaps	0x50(%rsp),%xmm10
	movaps	0x60(%rsp),%xmm11
	movaps	0x70(%rsp),%xmm12
	movaps	0x80(%rsp),%xmm13
	movaps	0x90(%rsp),%xmm14
	movaps	0xa0(%rsp),%xmm15
	lea	0xb8(%rsp),%rsp
.Lctr32_epilogue:
___
$code.=<<___;
.Lctr32_abort:
	ret
.cfi_endproc
.size	${PREFIX}_ctr32_encrypt_blocks,.-${PREFIX}_ctr32_encrypt_blocks
___
}
$code.=<<___;
##
##  _aes_preheat
##
##  Fills register %r10 -> .aes_consts (so you can -fPIC)
##  and %xmm9-%xmm15 as specified below.
##
.type	_vpaes_preheat,\@abi-omnipotent
.align	16
_vpaes_preheat:
.cfi_startproc
	lea	.Lk_s0F(%rip), %r10
	movdqa	-0x20(%r10), %xmm10	# .Lk_inv
	movdqa	-0x10(%r10), %xmm11	# .Lk_inv+16
	movdqa	0x00(%r10), %xmm9	# .Lk_s0F
	movdqa	0x30(%r10), %xmm13	# .Lk_sb1
	movdqa	0x40(%r10), %xmm12	# .Lk_sb1+16
	movdqa	0x50(%r10), %xmm15	# .Lk_sb2
	movdqa	0x60(%r10), %xmm14	# .Lk_sb2+16
	ret
.cfi_endproc
.size	_vpaes_preheat,.-_vpaes_preheat
########################################################
##                                                    ##
##                     Constants                      ##
##                                                    ##
########################################################
.type	_vpaes_consts,\@object
.section .rodata
.align	64
_vpaes_consts:
.Lk_inv:	# inv, inva
	.quad	0x0E05060F0D080180, 0x040703090A0B0C02
	.quad	0x01040A060F0B0780, 0x030D0E0C02050809

.Lk_s0F:	# s0F
	.quad	0x0F0F0F0F0F0F0F0F, 0x0F0F0F0F0F0F0F0F

.Lk_ipt:	# input transform (lo, hi)
	.quad	0xC2B2E8985A2A7000, 0xCABAE09052227808
	.quad	0x4C01307D317C4D00, 0xCD80B1FCB0FDCC81

.Lk_sb1:	# sb1u, sb1t
	.quad	0xB19BE18FCB503E00, 0xA5DF7A6E142AF544
	.quad	0x3618D415FAE22300, 0x3BF7CCC10D2ED9EF
.Lk_sb2:	# sb2u, sb2t
	.quad	0xE27A93C60B712400, 0x5EB7E955BC982FCD
	.quad	0x69EB88400AE12900, 0xC2A163C8AB82234A
.Lk_sbo:	# sbou, sbot
	.quad	0xD0D26D176FBDC700, 0x15AABF7AC502A878
	.quad	0xCFE474A55FBB6A00, 0x8E1E90D1412B35FA

.Lk_mc_forward:	# mc_forward
	.quad	0x0407060500030201, 0x0C0F0E0D080B0A09
	.quad	0x080B0A0904070605, 0x000302010C0F0E0D
	.quad	0x0C0F0E0D080B0A09, 0x0407060500030201
	.quad	0x000302010C0F0E0D, 0x080B0A0904070605

.Lk_mc_backward:# mc_backward
	.quad	0x0605040702010003, 0x0E0D0C0F0A09080B
	.quad	0x020100030E0D0C0F, 0x0A09080B06050407
	.quad	0x0E0D0C0F0A09080B, 0x0605040702010003
	.quad	0x0A09080B06050407, 0x020100030E0D0C0F

.Lk_sr:		# sr
	.quad	0x0706050403020100, 0x0F0E0D0C0B0A0908
	.quad	0x030E09040F0A0500, 0x0B06010C07020D08
	.quad	0x0F060D040B020900, 0x070E050C030A0108
	.quad	0x0B0E0104070A0D00, 0x0306090C0F020508

.Lk_rcon:	# rcon
	.quad	0x1F8391B9AF9DEEB6, 0x702A98084D7C7D81

.Lk_s63:	# s63: all equal to 0x63 transformed
	.quad	0x5B5B5B5B5B5B5B5B, 0x5B5B5B5B5B5B5B5B

.Lk_opt:	# output transform
	.quad	0xFF9F4929D6B66000, 0xF7974121DEBE6808
	.quad	0x01EDBD5150BCEC00, 0xE10D5DB1B05C0CE0

.Lk_deskew:	# deskew tables: inverts the sbox's "skew"
	.quad	0x07E4A34047A4E300, 0x1DFEB95A5DBEF91A
	.quad	0x5F36B5DC83EA6900, 0x2841C2ABF49D1E77

# .Lrev_ctr is a permutation which byte-swaps the counter portion of the IV.
.Lrev_ctr:
	.quad	0x0706050403020100, 0x0c0d0e0f0b0a0908
# .Lctr_add_* may be added to a byte-swapped xmm register to increment the
# counter. The register must be byte-swapped again to form the actual input.
.Lctr_add_one:
	.quad	0x0000000000000000, 0x0000000100000000
.Lctr_add_two:
	.quad	0x0000000000000000, 0x0000000200000000

.asciz	"Vector Permutation AES for x86_64/SSSE3, Mike Hamburg (Stanford University)"
.align	64
.size	_vpaes_consts,.-_vpaes_consts
.text
___

if ($win64) {
# EXCEPTION_DISPOSITION handler (EXCEPTION_RECORD *rec,ULONG64 frame,
#		CONTEXT *context,DISPATCHER_CONTEXT *disp)
$rec="%rcx";
$frame="%rdx";
$context="%r8";
$disp="%r9";

$code.=<<___;
.extern	__imp_RtlVirtualUnwind
.type	se_handler,\@abi-omnipotent
.align	16
se_handler:
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
	jb	.Lin_prologue

	mov	152($context), 

	mov	4( 
	lea	( 
	cmp	
	jae	.Lin_prologue

	lea	16( 
	lea	512($context), 
	mov	\$20, 
	.long	0xa548f3fc		# cld; rep movsq
	lea	0xb8( 

.Lin_prologue:
	mov	8( 
	mov	16( 
	mov	
	mov	
	mov	

	mov	40($disp), 
	mov	$context, 
	mov	\$`1232/8`, 
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
.size	se_handler,.-se_handler

.section	.pdata
.align	4
	.rva	.LSEH_begin_${PREFIX}_set_encrypt_key
	.rva	.LSEH_end_${PREFIX}_set_encrypt_key
	.rva	.LSEH_info_${PREFIX}_set_encrypt_key

	.rva	.LSEH_begin_${PREFIX}_ctr32_encrypt_blocks
	.rva	.LSEH_end_${PREFIX}_ctr32_encrypt_blocks
	.rva	.LSEH_info_${PREFIX}_ctr32_encrypt_blocks

.section	.xdata
.align	8
.LSEH_info_${PREFIX}_set_encrypt_key:
	.byte	9,0,0,0
	.rva	se_handler
	.rva	.Lenc_key_body,.Lenc_key_epilogue	# HandlerData[]
.LSEH_info_${PREFIX}_ctr32_encrypt_blocks:
	.byte	9,0,0,0
	.rva	se_handler
	.rva	.Lctr32_body,.Lctr32_epilogue		# HandlerData[]
___
}

$code =~ s/\`([^\`]*)\`/eval($1)/gem;

print $code;

close STDOUT or die "error closing STDOUT: $!";
