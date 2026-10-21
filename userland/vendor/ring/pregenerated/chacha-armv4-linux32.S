#include <ring-core/asm_base.h>

#if !defined(OPENSSL_NO_ASM) && defined(OPENSSL_ARM) && defined(__ELF__)
nrnrnrnnnrrnnnrn.arch	armv7-a

.text
#if defined(__thumb2__) || defined(__clang__)
.syntax	unified
#endif
#if defined(__thumb2__)
.thumb
#else
.code	32
#endif

#if defined(__thumb2__) || defined(__clang__)
#define ldrhsb	ldrbhs
#endif

.align	5
.Lsigma:
.long	0x61707865,0x3320646e,0x79622d32,0x6b206574	nnnr.Lone:
.long	1,0,0,0

.globl	ChaCha20_ctr32_nohw
.hidden	ChaCha20_ctr32_nohw
.type	ChaCha20_ctr32_nohw,%function
.align	5
ChaCha20_ctr32_nohw:
	ldr	r12,[sp,#0]		nrnrnnn	stmdb	sp!,{r0,r1,r2,r4-r11,lr}
	adr	r14,.Lsigma
	ldmia	r12,{r4,r5,r6,r7}		nrnnn	sub	sp,sp,#4*(16)		r	stmdb	sp!,{r4,r5,r6,r7}		nrnnn	ldmia	r3,{r4,r5,r6,r7,r8,r9,r10,r11}			ldmia	r14,{r0,r1,r2,r3}			stmdb	sp!,{r4,r5,r6,r7,r8,r9,r10,r11}			stmdb	sp!,{r0,r1,r2,r3}			str	r10,[sp,#4*(16+10)]	r	str	r11,[sp,#4*(16+11)]	r	b	.Loop_outer_enter

.align	4
.Loop_outer:
	ldmia	sp,{r0,r1,r2,r3,r4,r5,r6,r7,r8,r9}		r	str	r11,[sp,#4*(32+2)]	n	str	r12,  [sp,#4*(32+1)]	n	str	r14,  [sp,#4*(32+0)]	.Loop_outer_enter:
	ldr	r11, [sp,#4*(15)]
	ldr	r12,[sp,#4*(12)]		ldr	r10, [sp,#4*(13)]
	ldr	r14,[sp,#4*(14)]
	str	r11, [sp,#4*(16+15)]
	mov	r11,#10
	b	.Loop

.align	4
.Loop:
	subs	r11,r11,#1
	add	r0,r0,r4
	mov	r12,r12,ror#16
	add	r1,r1,r5
	mov	r10,r10,ror#16
	eor	r12,r12,r0,ror#16
	eor	r10,r10,r1,ror#16
	add	r8,r8,r12
	mov	r4,r4,ror#20
	add	r9,r9,r10
	mov	r5,r5,ror#20
	eor	r4,r4,r8,ror#20
	eor	r5,r5,r9,ror#20
	add	r0,r0,r4
	mov	r12,r12,ror#24
	add	r1,r1,r5
	mov	r10,r10,ror#24
	eor	r12,r12,r0,ror#24
	eor	r10,r10,r1,ror#24
	add	r8,r8,r12
	mov	r4,r4,ror#25
	add	r9,r9,r10
	mov	r5,r5,ror#25
	str	r10,[sp,#4*(16+13)]
	ldr	r10,[sp,#4*(16+15)]
	eor	r4,r4,r8,ror#25
	eor	r5,r5,r9,ror#25
	str	r8,[sp,#4*(16+8)]
	ldr	r8,[sp,#4*(16+10)]
	add	r2,r2,r6
	mov	r14,r14,ror#16
	str	r9,[sp,#4*(16+9)]
	ldr	r9,[sp,#4*(16+11)]
	add	r3,r3,r7
	mov	r10,r10,ror#16
	eor	r14,r14,r2,ror#16
	eor	r10,r10,r3,ror#16
	add	r8,r8,r14
	mov	r6,r6,ror#20
	add	r9,r9,r10
	mov	r7,r7,ror#20
	eor	r6,r6,r8,ror#20
	eor	r7,r7,r9,ror#20
	add	r2,r2,r6
	mov	r14,r14,ror#24
	add	r3,r3,r7
	mov	r10,r10,ror#24
	eor	r14,r14,r2,ror#24
	eor	r10,r10,r3,ror#24
	add	r8,r8,r14
	mov	r6,r6,ror#25
	add	r9,r9,r10
	mov	r7,r7,ror#25
	eor	r6,r6,r8,ror#25
	eor	r7,r7,r9,ror#25
	add	r0,r0,r5
	mov	r10,r10,ror#16
	add	r1,r1,r6
	mov	r12,r12,ror#16
	eor	r10,r10,r0,ror#16
	eor	r12,r12,r1,ror#16
	add	r8,r8,r10
	mov	r5,r5,ror#20
	add	r9,r9,r12
	mov	r6,r6,ror#20
	eor	r5,r5,r8,ror#20
	eor	r6,r6,r9,ror#20
	add	r0,r0,r5
	mov	r10,r10,ror#24
	add	r1,r1,r6
	mov	r12,r12,ror#24
	eor	r10,r10,r0,ror#24
	eor	r12,r12,r1,ror#24
	add	r8,r8,r10
	mov	r5,r5,ror#25
	str	r10,[sp,#4*(16+15)]
	ldr	r10,[sp,#4*(16+13)]
	add	r9,r9,r12
	mov	r6,r6,ror#25
	eor	r5,r5,r8,ror#25
	eor	r6,r6,r9,ror#25
	str	r8,[sp,#4*(16+10)]
	ldr	r8,[sp,#4*(16+8)]
	add	r2,r2,r7
	mov	r10,r10,ror#16
	str	r9,[sp,#4*(16+11)]
	ldr	r9,[sp,#4*(16+9)]
	add	r3,r3,r4
	mov	r14,r14,ror#16
	eor	r10,r10,r2,ror#16
	eor	r14,r14,r3,ror#16
	add	r8,r8,r10
	mov	r7,r7,ror#20
	add	r9,r9,r14
	mov	r4,r4,ror#20
	eor	r7,r7,r8,ror#20
	eor	r4,r4,r9,ror#20
	add	r2,r2,r7
	mov	r10,r10,ror#24
	add	r3,r3,r4
	mov	r14,r14,ror#24
	eor	r10,r10,r2,ror#24
	eor	r14,r14,r3,ror#24
	add	r8,r8,r10
	mov	r7,r7,ror#25
	add	r9,r9,r14
	mov	r4,r4,ror#25
	eor	r7,r7,r8,ror#25
	eor	r4,r4,r9,ror#25
	bne	.Loop

	ldr	r11,[sp,#4*(32+2)]	n
	str	r8, [sp,#4*(16+8)]	r	str	r9, [sp,#4*(16+9)]
	str	r12,[sp,#4*(16+12)]
	str	r10, [sp,#4*(16+13)]
	str	r14,[sp,#4*(16+14)]

	nrrn	rnn
	cmp	r11,#64		n#ifdef	__thumb2__
	itete	lo
#endif
	addlo	r12,sp,#4*(0)		rr	ldrhs	r12,[sp,#4*(32+1)]	n	addlo	r14,sp,#4*(0)		rr	ldrhs	r14,[sp,#4*(32+0)]	
	ldr	r8,[sp,#4*(0)]	r	ldr	r9,[sp,#4*(1)]

#if __ARM_ARCH>=6 || !defined(__ARMEB__)
# if __ARM_ARCH<7
	orr	r10,r12,r14
	tst	r10,#3		rnnn	ldr	r10,[sp,#4*(2)]
	bne	.Lunaligned
	cmp	r11,#64		rr# else
	ldr	r10,[sp,#4*(2)]
# endif
	ldr	r11,[sp,#4*(3)]

	add	r0,r0,r8	r	add	r1,r1,r9
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r8,[r12],#16		n	ldrhs	r9,[r12,#-12]

	add	r2,r2,r10
	add	r3,r3,r11
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r10,[r12,#-8]
	ldrhs	r11,[r12,#-4]
# if __ARM_ARCH>=6 && defined(__ARMEB__)
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
# endif
# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r0,r0,r8	rn	eorhs	r1,r1,r9
	add	r8,sp,#4*(4)
	str	r0,[r14],#16		r# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r2,r2,r10
	eorhs	r3,r3,r11
	ldmia	r8,{r8,r9,r10,r11}	r	str	r1,[r14,#-12]
	str	r2,[r14,#-8]
	str	r3,[r14,#-4]

	add	r4,r4,r8	r	add	r5,r5,r9
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r8,[r12],#16		n	ldrhs	r9,[r12,#-12]
	add	r6,r6,r10
	add	r7,r7,r11
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r10,[r12,#-8]
	ldrhs	r11,[r12,#-4]
# if __ARM_ARCH>=6 && defined(__ARMEB__)
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r4,r4,r8
	eorhs	r5,r5,r9
	add	r8,sp,#4*(8)
	str	r4,[r14],#16		r# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r6,r6,r10
	eorhs	r7,r7,r11
	str	r5,[r14,#-12]
	ldmia	r8,{r8,r9,r10,r11}	r	str	r6,[r14,#-8]
	add	r0,sp,#4*(16+8)
	str	r7,[r14,#-4]

	ldmia	r0,{r0,r1,r2,r3,r4,r5,r6,r7}	n
	add	r0,r0,r8	r	add	r1,r1,r9
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r8,[r12],#16		n	ldrhs	r9,[r12,#-12]
# ifdef	__thumb2__
	itt	hi
# endif
	strhi	r10,[sp,#4*(16+10)]	r	strhi	r11,[sp,#4*(16+11)]	r	add	r2,r2,r10
	add	r3,r3,r11
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r10,[r12,#-8]
	ldrhs	r11,[r12,#-4]
# if __ARM_ARCH>=6 && defined(__ARMEB__)
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
# endif
# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r0,r0,r8
	eorhs	r1,r1,r9
	add	r8,sp,#4*(12)
	str	r0,[r14],#16		r# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r2,r2,r10
	eorhs	r3,r3,r11
	str	r1,[r14,#-12]
	ldmia	r8,{r8,r9,r10,r11}	r	str	r2,[r14,#-8]
	str	r3,[r14,#-4]

	add	r4,r4,r8	r	add	r5,r5,r9
# ifdef	__thumb2__
	itt	hi
# endif
	addhi	r8,r8,#1		nnr	strhi	r8,[sp,#4*(12)]	nnr# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r8,[r12],#16		n	ldrhs	r9,[r12,#-12]
	add	r6,r6,r10
	add	r7,r7,r11
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhs	r10,[r12,#-8]
	ldrhs	r11,[r12,#-4]
# if __ARM_ARCH>=6 && defined(__ARMEB__)
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r4,r4,r8
	eorhs	r5,r5,r9
# ifdef	__thumb2__
	it	ne
# endif
	ldrne	r8,[sp,#4*(32+2)]	rn# ifdef	__thumb2__
	itt	hs
# endif
	eorhs	r6,r6,r10
	eorhs	r7,r7,r11
	str	r4,[r14],#16		r	str	r5,[r14,#-12]
# ifdef	__thumb2__
	it	hs
# endif
	subhs	r11,r8,#64		n	str	r6,[r14,#-8]
	str	r7,[r14,#-4]
	bhi	.Loop_outer

	beq	.Ldone
# if __ARM_ARCH<7
	b	.Ltail

.align	4
.Lunaligned:@ unaligned endian-neutral path
	cmp	r11,#64		rr# endif
#endif
#if __ARM_ARCH<7
	ldr	r11,[sp,#4*(3)]
	add	r0,r0,r8		r	add	r1,r1,r9
	add	r2,r2,r10
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r8,r8,r8		rr	ldrhsb	r8,[r12],#16			n	eorlo	r9,r9,r9
	ldrhsb	r9,[r12,#-12]

	add	r3,r3,r11
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r10,r10,r10
	ldrhsb	r10,[r12,#-8]
	eorlo	r11,r11,r11
	ldrhsb	r11,[r12,#-4]

	eor	r0,r8,r0		rnrr	eor	r1,r9,r1
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-15]		rn	ldrhsb	r9,[r12,#-11]
	eor	r2,r10,r2
	strb	r0,[r14],#16		r	eor	r3,r11,r3
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-7]
	ldrhsb	r11,[r12,#-3]
	strb	r1,[r14,#-12]
	eor	r0,r8,r0,lsr#8
	strb	r2,[r14,#-8]
	eor	r1,r9,r1,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-14]		rn	ldrhsb	r9,[r12,#-10]
	strb	r3,[r14,#-4]
	eor	r2,r10,r2,lsr#8
	strb	r0,[r14,#-15]
	eor	r3,r11,r3,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-6]
	ldrhsb	r11,[r12,#-2]
	strb	r1,[r14,#-11]
	eor	r0,r8,r0,lsr#8
	strb	r2,[r14,#-7]
	eor	r1,r9,r1,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-13]		rn	ldrhsb	r9,[r12,#-9]
	strb	r3,[r14,#-3]
	eor	r2,r10,r2,lsr#8
	strb	r0,[r14,#-14]
	eor	r3,r11,r3,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-5]
	ldrhsb	r11,[r12,#-1]
	strb	r1,[r14,#-10]
	strb	r2,[r14,#-6]
	eor	r0,r8,r0,lsr#8
	strb	r3,[r14,#-2]
	eor	r1,r9,r1,lsr#8
	strb	r0,[r14,#-13]
	eor	r2,r10,r2,lsr#8
	strb	r1,[r14,#-9]
	eor	r3,r11,r3,lsr#8
	strb	r2,[r14,#-5]
	strb	r3,[r14,#-1]
	add	r8,sp,#4*(4+0)
	ldmia	r8,{r8,r9,r10,r11}		r	add	r0,sp,#4*(16+8)
	add	r4,r4,r8		r	add	r5,r5,r9
	add	r6,r6,r10
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r8,r8,r8		rr	ldrhsb	r8,[r12],#16			n	eorlo	r9,r9,r9
	ldrhsb	r9,[r12,#-12]

	add	r7,r7,r11
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r10,r10,r10
	ldrhsb	r10,[r12,#-8]
	eorlo	r11,r11,r11
	ldrhsb	r11,[r12,#-4]

	eor	r4,r8,r4		rnrr	eor	r5,r9,r5
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-15]		rn	ldrhsb	r9,[r12,#-11]
	eor	r6,r10,r6
	strb	r4,[r14],#16		r	eor	r7,r11,r7
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-7]
	ldrhsb	r11,[r12,#-3]
	strb	r5,[r14,#-12]
	eor	r4,r8,r4,lsr#8
	strb	r6,[r14,#-8]
	eor	r5,r9,r5,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-14]		rn	ldrhsb	r9,[r12,#-10]
	strb	r7,[r14,#-4]
	eor	r6,r10,r6,lsr#8
	strb	r4,[r14,#-15]
	eor	r7,r11,r7,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-6]
	ldrhsb	r11,[r12,#-2]
	strb	r5,[r14,#-11]
	eor	r4,r8,r4,lsr#8
	strb	r6,[r14,#-7]
	eor	r5,r9,r5,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-13]		rn	ldrhsb	r9,[r12,#-9]
	strb	r7,[r14,#-3]
	eor	r6,r10,r6,lsr#8
	strb	r4,[r14,#-14]
	eor	r7,r11,r7,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-5]
	ldrhsb	r11,[r12,#-1]
	strb	r5,[r14,#-10]
	strb	r6,[r14,#-6]
	eor	r4,r8,r4,lsr#8
	strb	r7,[r14,#-2]
	eor	r5,r9,r5,lsr#8
	strb	r4,[r14,#-13]
	eor	r6,r10,r6,lsr#8
	strb	r5,[r14,#-9]
	eor	r7,r11,r7,lsr#8
	strb	r6,[r14,#-5]
	strb	r7,[r14,#-1]
	add	r8,sp,#4*(4+4)
	ldmia	r8,{r8,r9,r10,r11}		r	ldmia	r0,{r0,r1,r2,r3,r4,r5,r6,r7}		n# ifdef	__thumb2__
	itt	hi
# endif
	strhi	r10,[sp,#4*(16+10)]		r	strhi	r11,[sp,#4*(16+11)]		r	add	r0,r0,r8		r	add	r1,r1,r9
	add	r2,r2,r10
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r8,r8,r8		rr	ldrhsb	r8,[r12],#16			n	eorlo	r9,r9,r9
	ldrhsb	r9,[r12,#-12]

	add	r3,r3,r11
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r10,r10,r10
	ldrhsb	r10,[r12,#-8]
	eorlo	r11,r11,r11
	ldrhsb	r11,[r12,#-4]

	eor	r0,r8,r0		rnrr	eor	r1,r9,r1
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-15]		rn	ldrhsb	r9,[r12,#-11]
	eor	r2,r10,r2
	strb	r0,[r14],#16		r	eor	r3,r11,r3
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-7]
	ldrhsb	r11,[r12,#-3]
	strb	r1,[r14,#-12]
	eor	r0,r8,r0,lsr#8
	strb	r2,[r14,#-8]
	eor	r1,r9,r1,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-14]		rn	ldrhsb	r9,[r12,#-10]
	strb	r3,[r14,#-4]
	eor	r2,r10,r2,lsr#8
	strb	r0,[r14,#-15]
	eor	r3,r11,r3,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-6]
	ldrhsb	r11,[r12,#-2]
	strb	r1,[r14,#-11]
	eor	r0,r8,r0,lsr#8
	strb	r2,[r14,#-7]
	eor	r1,r9,r1,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-13]		rn	ldrhsb	r9,[r12,#-9]
	strb	r3,[r14,#-3]
	eor	r2,r10,r2,lsr#8
	strb	r0,[r14,#-14]
	eor	r3,r11,r3,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-5]
	ldrhsb	r11,[r12,#-1]
	strb	r1,[r14,#-10]
	strb	r2,[r14,#-6]
	eor	r0,r8,r0,lsr#8
	strb	r3,[r14,#-2]
	eor	r1,r9,r1,lsr#8
	strb	r0,[r14,#-13]
	eor	r2,r10,r2,lsr#8
	strb	r1,[r14,#-9]
	eor	r3,r11,r3,lsr#8
	strb	r2,[r14,#-5]
	strb	r3,[r14,#-1]
	add	r8,sp,#4*(4+8)
	ldmia	r8,{r8,r9,r10,r11}		r	add	r4,r4,r8		r# ifdef	__thumb2__
	itt	hi
# endif
	addhi	r8,r8,#1			nnr	strhi	r8,[sp,#4*(12)]		nnr	add	r5,r5,r9
	add	r6,r6,r10
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r8,r8,r8		rr	ldrhsb	r8,[r12],#16			n	eorlo	r9,r9,r9
	ldrhsb	r9,[r12,#-12]

	add	r7,r7,r11
# ifdef	__thumb2__
	itete	lo
# endif
	eorlo	r10,r10,r10
	ldrhsb	r10,[r12,#-8]
	eorlo	r11,r11,r11
	ldrhsb	r11,[r12,#-4]

	eor	r4,r8,r4		rnrr	eor	r5,r9,r5
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-15]		rn	ldrhsb	r9,[r12,#-11]
	eor	r6,r10,r6
	strb	r4,[r14],#16		r	eor	r7,r11,r7
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-7]
	ldrhsb	r11,[r12,#-3]
	strb	r5,[r14,#-12]
	eor	r4,r8,r4,lsr#8
	strb	r6,[r14,#-8]
	eor	r5,r9,r5,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-14]		rn	ldrhsb	r9,[r12,#-10]
	strb	r7,[r14,#-4]
	eor	r6,r10,r6,lsr#8
	strb	r4,[r14,#-15]
	eor	r7,r11,r7,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-6]
	ldrhsb	r11,[r12,#-2]
	strb	r5,[r14,#-11]
	eor	r4,r8,r4,lsr#8
	strb	r6,[r14,#-7]
	eor	r5,r9,r5,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r8,[r12,#-13]		rn	ldrhsb	r9,[r12,#-9]
	strb	r7,[r14,#-3]
	eor	r6,r10,r6,lsr#8
	strb	r4,[r14,#-14]
	eor	r7,r11,r7,lsr#8
# ifdef	__thumb2__
	itt	hs
# endif
	ldrhsb	r10,[r12,#-5]
	ldrhsb	r11,[r12,#-1]
	strb	r5,[r14,#-10]
	strb	r6,[r14,#-6]
	eor	r4,r8,r4,lsr#8
	strb	r7,[r14,#-2]
	eor	r5,r9,r5,lsr#8
	strb	r4,[r14,#-13]
	eor	r6,r10,r6,lsr#8
	strb	r5,[r14,#-9]
	eor	r7,r11,r7,lsr#8
	strb	r6,[r14,#-5]
	strb	r7,[r14,#-1]
# ifdef	__thumb2__
	it	ne
# endif
	ldrne	r8,[sp,#4*(32+2)]		rn# ifdef	__thumb2__
	it	hs
# endif
	subhs	r11,r8,#64			n	bhi	.Loop_outer

	beq	.Ldone
#endif

.Ltail:
	ldr	r12,[sp,#4*(32+1)]	n	add	r9,sp,#4*(0)
	ldr	r14,[sp,#4*(32+0)]	
.Loop_tail:
	ldrb	r10,[r9],#1	rrn	ldrb	r11,[r12],#1		rn	subs	r8,r8,#1
	eor	r11,r11,r10
	strb	r11,[r14],#1		r	bne	.Loop_tail

.Ldone:
	add	sp,sp,#4*(32+3)
	ldmia	sp!,{r4,r5,r6,r7,r8,r9,r10,r11,pc}
.size	ChaCha20_ctr32_nohw,.-ChaCha20_ctr32_nohw
#if __ARM_MAX_ARCH__>=7
.arch	armv7-a
.fpu	neon

.globl	ChaCha20_ctr32_neon
.hidden	ChaCha20_ctr32_neon
.type	ChaCha20_ctr32_neon,%function
.align	5
ChaCha20_ctr32_neon:
	ldr	r12,[sp,#0]		nrnrnnn	stmdb	sp!,{r0,r1,r2,r4-r11,lr}
	adr	r14,.Lsigma
	vstmdb	sp!,{d8,d9,d10,d11,d12,d13,d14,d15}			stmdb	sp!,{r0,r1,r2,r3}

	vld1.32	{q1,q2},[r3]			ldmia	r3,{r4,r5,r6,r7,r8,r9,r10,r11}		
	sub	sp,sp,#4*(16+16)
	vld1.32	{q3},[r12]		nrnnn	add	r12,sp,#4*8
	ldmia	r14,{r0,r1,r2,r3}			vld1.32	{q0},[r14]!			vld1.32	{q12},[r14]		n	vst1.32	{q2,q3},[r12]		nrnn	vst1.32	{q0,q1},[sp]		
	str	r10,[sp,#4*(16+10)]	r	str	r11,[sp,#4*(16+11)]	r	vshl.i32	d26,d24,#1		vstr	d24,[sp,#4*(16+0)]
	vshl.i32	d28,d24,#2	r	vstr	d26,[sp,#4*(16+2)]
	vmov	q4,q0
	vstr	d28,[sp,#4*(16+4)]
	vmov	q8,q0
	vmov	q5,q1
	vmov	q9,q1
	b	.Loop_neon_enter

.align	4
.Loop_neon_outer:
	ldmia	sp,{r0,r1,r2,r3,r4,r5,r6,r7,r8,r9}		r	cmp	r11,#64*2		n	bls	.Lbreak_neon		nrn	vmov	q4,q0
	str	r11,[sp,#4*(32+2)]	n	vmov	q8,q0
	str	r12,  [sp,#4*(32+1)]	n	vmov	q5,q1
	str	r14,  [sp,#4*(32+0)]		vmov	q9,q1
.Loop_neon_enter:
	ldr	r11, [sp,#4*(15)]
	vadd.i32	q7,q3,q12		nr	ldr	r12,[sp,#4*(12)]		vmov	q6,q2
	ldr	r10, [sp,#4*(13)]
	vmov	q10,q2
	ldr	r14,[sp,#4*(14)]
	vadd.i32	q11,q7,q12		nr	str	r11, [sp,#4*(16+15)]
	mov	r11,#10
	add	r12,r12,#3	nr	b	.Loop_neon

.align	4
.Loop_neon:
	subs	r11,r11,#1
	vadd.i32	q0,q0,q1
	add	r0,r0,r4
	vadd.i32	q4,q4,q5
	mov	r12,r12,ror#16
	vadd.i32	q8,q8,q9
	add	r1,r1,r5
	veor	q3,q3,q0
	mov	r10,r10,ror#16
	veor	q7,q7,q4
	eor	r12,r12,r0,ror#16
	veor	q11,q11,q8
	eor	r10,r10,r1,ror#16
	vrev32.16	q3,q3
	add	r8,r8,r12
	vrev32.16	q7,q7
	mov	r4,r4,ror#20
	vrev32.16	q11,q11
	add	r9,r9,r10
	vadd.i32	q2,q2,q3
	mov	r5,r5,ror#20
	vadd.i32	q6,q6,q7
	eor	r4,r4,r8,ror#20
	vadd.i32	q10,q10,q11
	eor	r5,r5,r9,ror#20
	veor	q12,q1,q2
	add	r0,r0,r4
	veor	q13,q5,q6
	mov	r12,r12,ror#24
	veor	q14,q9,q10
	add	r1,r1,r5
	vshr.u32	q1,q12,#20
	mov	r10,r10,ror#24
	vshr.u32	q5,q13,#20
	eor	r12,r12,r0,ror#24
	vshr.u32	q9,q14,#20
	eor	r10,r10,r1,ror#24
	vsli.32	q1,q12,#12
	add	r8,r8,r12
	vsli.32	q5,q13,#12
	mov	r4,r4,ror#25
	vsli.32	q9,q14,#12
	add	r9,r9,r10
	vadd.i32	q0,q0,q1
	mov	r5,r5,ror#25
	vadd.i32	q4,q4,q5
	str	r10,[sp,#4*(16+13)]
	vadd.i32	q8,q8,q9
	ldr	r10,[sp,#4*(16+15)]
	veor	q12,q3,q0
	eor	r4,r4,r8,ror#25
	veor	q13,q7,q4
	eor	r5,r5,r9,ror#25
	veor	q14,q11,q8
	str	r8,[sp,#4*(16+8)]
	vshr.u32	q3,q12,#24
	ldr	r8,[sp,#4*(16+10)]
	vshr.u32	q7,q13,#24
	add	r2,r2,r6
	vshr.u32	q11,q14,#24
	mov	r14,r14,ror#16
	vsli.32	q3,q12,#8
	str	r9,[sp,#4*(16+9)]
	vsli.32	q7,q13,#8
	ldr	r9,[sp,#4*(16+11)]
	vsli.32	q11,q14,#8
	add	r3,r3,r7
	vadd.i32	q2,q2,q3
	mov	r10,r10,ror#16
	vadd.i32	q6,q6,q7
	eor	r14,r14,r2,ror#16
	vadd.i32	q10,q10,q11
	eor	r10,r10,r3,ror#16
	veor	q12,q1,q2
	add	r8,r8,r14
	veor	q13,q5,q6
	mov	r6,r6,ror#20
	veor	q14,q9,q10
	add	r9,r9,r10
	vshr.u32	q1,q12,#25
	mov	r7,r7,ror#20
	vshr.u32	q5,q13,#25
	eor	r6,r6,r8,ror#20
	vshr.u32	q9,q14,#25
	eor	r7,r7,r9,ror#20
	vsli.32	q1,q12,#7
	add	r2,r2,r6
	vsli.32	q5,q13,#7
	mov	r14,r14,ror#24
	vsli.32	q9,q14,#7
	add	r3,r3,r7
	vext.8	q2,q2,q2,#8
	mov	r10,r10,ror#24
	vext.8	q6,q6,q6,#8
	eor	r14,r14,r2,ror#24
	vext.8	q10,q10,q10,#8
	eor	r10,r10,r3,ror#24
	vext.8	q1,q1,q1,#4
	add	r8,r8,r14
	vext.8	q5,q5,q5,#4
	mov	r6,r6,ror#25
	vext.8	q9,q9,q9,#4
	add	r9,r9,r10
	vext.8	q3,q3,q3,#12
	mov	r7,r7,ror#25
	vext.8	q7,q7,q7,#12
	eor	r6,r6,r8,ror#25
	vext.8	q11,q11,q11,#12
	eor	r7,r7,r9,ror#25
	vadd.i32	q0,q0,q1
	add	r0,r0,r5
	vadd.i32	q4,q4,q5
	mov	r10,r10,ror#16
	vadd.i32	q8,q8,q9
	add	r1,r1,r6
	veor	q3,q3,q0
	mov	r12,r12,ror#16
	veor	q7,q7,q4
	eor	r10,r10,r0,ror#16
	veor	q11,q11,q8
	eor	r12,r12,r1,ror#16
	vrev32.16	q3,q3
	add	r8,r8,r10
	vrev32.16	q7,q7
	mov	r5,r5,ror#20
	vrev32.16	q11,q11
	add	r9,r9,r12
	vadd.i32	q2,q2,q3
	mov	r6,r6,ror#20
	vadd.i32	q6,q6,q7
	eor	r5,r5,r8,ror#20
	vadd.i32	q10,q10,q11
	eor	r6,r6,r9,ror#20
	veor	q12,q1,q2
	add	r0,r0,r5
	veor	q13,q5,q6
	mov	r10,r10,ror#24
	veor	q14,q9,q10
	add	r1,r1,r6
	vshr.u32	q1,q12,#20
	mov	r12,r12,ror#24
	vshr.u32	q5,q13,#20
	eor	r10,r10,r0,ror#24
	vshr.u32	q9,q14,#20
	eor	r12,r12,r1,ror#24
	vsli.32	q1,q12,#12
	add	r8,r8,r10
	vsli.32	q5,q13,#12
	mov	r5,r5,ror#25
	vsli.32	q9,q14,#12
	str	r10,[sp,#4*(16+15)]
	vadd.i32	q0,q0,q1
	ldr	r10,[sp,#4*(16+13)]
	vadd.i32	q4,q4,q5
	add	r9,r9,r12
	vadd.i32	q8,q8,q9
	mov	r6,r6,ror#25
	veor	q12,q3,q0
	eor	r5,r5,r8,ror#25
	veor	q13,q7,q4
	eor	r6,r6,r9,ror#25
	veor	q14,q11,q8
	str	r8,[sp,#4*(16+10)]
	vshr.u32	q3,q12,#24
	ldr	r8,[sp,#4*(16+8)]
	vshr.u32	q7,q13,#24
	add	r2,r2,r7
	vshr.u32	q11,q14,#24
	mov	r10,r10,ror#16
	vsli.32	q3,q12,#8
	str	r9,[sp,#4*(16+11)]
	vsli.32	q7,q13,#8
	ldr	r9,[sp,#4*(16+9)]
	vsli.32	q11,q14,#8
	add	r3,r3,r4
	vadd.i32	q2,q2,q3
	mov	r14,r14,ror#16
	vadd.i32	q6,q6,q7
	eor	r10,r10,r2,ror#16
	vadd.i32	q10,q10,q11
	eor	r14,r14,r3,ror#16
	veor	q12,q1,q2
	add	r8,r8,r10
	veor	q13,q5,q6
	mov	r7,r7,ror#20
	veor	q14,q9,q10
	add	r9,r9,r14
	vshr.u32	q1,q12,#25
	mov	r4,r4,ror#20
	vshr.u32	q5,q13,#25
	eor	r7,r7,r8,ror#20
	vshr.u32	q9,q14,#25
	eor	r4,r4,r9,ror#20
	vsli.32	q1,q12,#7
	add	r2,r2,r7
	vsli.32	q5,q13,#7
	mov	r10,r10,ror#24
	vsli.32	q9,q14,#7
	add	r3,r3,r4
	vext.8	q2,q2,q2,#8
	mov	r14,r14,ror#24
	vext.8	q6,q6,q6,#8
	eor	r10,r10,r2,ror#24
	vext.8	q10,q10,q10,#8
	eor	r14,r14,r3,ror#24
	vext.8	q1,q1,q1,#12
	add	r8,r8,r10
	vext.8	q5,q5,q5,#12
	mov	r7,r7,ror#25
	vext.8	q9,q9,q9,#12
	add	r9,r9,r14
	vext.8	q3,q3,q3,#4
	mov	r4,r4,ror#25
	vext.8	q7,q7,q7,#4
	eor	r7,r7,r8,ror#25
	vext.8	q11,q11,q11,#4
	eor	r4,r4,r9,ror#25
	bne	.Loop_neon

	add	r11,sp,#32
	vld1.32	{q12,q13},[sp]		r	vld1.32	{q14,q15},[r11]

	ldr	r11,[sp,#4*(32+2)]	n
	str	r8, [sp,#4*(16+8)]	r	str	r9, [sp,#4*(16+9)]
	str	r12,[sp,#4*(16+12)]
	str	r10, [sp,#4*(16+13)]
	str	r14,[sp,#4*(16+14)]

	nrrn	rnn
	ldr	r12,[sp,#4*(32+1)]	n	ldr	r14,[sp,#4*(32+0)]	
	vadd.i32	q0,q0,q12		r	vadd.i32	q4,q4,q12
	vadd.i32	q8,q8,q12
	vldr	d24,[sp,#4*(16+0)]	n
	vadd.i32	q1,q1,q13
	vadd.i32	q5,q5,q13
	vadd.i32	q9,q9,q13
	vldr	d26,[sp,#4*(16+2)]	
	vadd.i32	q2,q2,q14
	vadd.i32	q6,q6,q14
	vadd.i32	q10,q10,q14
	vadd.i32	d14,d14,d24	nr	vadd.i32	d22,d22,d26	nr
	vadd.i32	q3,q3,q15
	vadd.i32	q7,q7,q15
	vadd.i32	q11,q11,q15

	cmp	r11,#64*4
	blo	.Ltail_neon

	vld1.8	{q12,q13},[r12]!	n	mov	r11,sp
	vld1.8	{q14,q15},[r12]!
	veor	q0,q0,q12		rn	veor	q1,q1,q13
	vld1.8	{q12,q13},[r12]!
	veor	q2,q2,q14
	veor	q3,q3,q15
	vld1.8	{q14,q15},[r12]!

	veor	q4,q4,q12
	vst1.8	{q0,q1},[r14]!	r	veor	q5,q5,q13
	vld1.8	{q12,q13},[r12]!
	veor	q6,q6,q14
	vst1.8	{q2,q3},[r14]!
	veor	q7,q7,q15
	vld1.8	{q14,q15},[r12]!

	veor	q8,q8,q12
	vld1.32	{q0,q1},[r11]!	rnrn	veor	d25,d25,d25
	vldr	d24,[sp,#4*(16+4)]	r	veor	q9,q9,q13
	vld1.32	{q2,q3},[r11]
	veor	q10,q10,q14
	vst1.8	{q4,q5},[r14]!
	veor	q11,q11,q15
	vst1.8	{q6,q7},[r14]!

	vadd.i32	d6,d6,d24	nnr	vldr	d24,[sp,#4*(16+0)]	n
	ldmia	sp,{r8,r9,r10,r11}	r	add	r0,r0,r8	r	ldr	r8,[r12],#16		n	vst1.8	{q8,q9},[r14]!
	add	r1,r1,r9
	ldr	r9,[r12,#-12]
	vst1.8	{q10,q11},[r14]!
	add	r2,r2,r10
	ldr	r10,[r12,#-8]
	add	r3,r3,r11
	ldr	r11,[r12,#-4]
# ifdef	__ARMEB__
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
# endif
	eor	r0,r0,r8	rn	add	r8,sp,#4*(4)
	eor	r1,r1,r9
	str	r0,[r14],#16		r	eor	r2,r2,r10
	str	r1,[r14,#-12]
	eor	r3,r3,r11
	ldmia	r8,{r8,r9,r10,r11}	r	str	r2,[r14,#-8]
	str	r3,[r14,#-4]

	add	r4,r4,r8	r	ldr	r8,[r12],#16		n	add	r5,r5,r9
	ldr	r9,[r12,#-12]
	add	r6,r6,r10
	ldr	r10,[r12,#-8]
	add	r7,r7,r11
	ldr	r11,[r12,#-4]
# ifdef	__ARMEB__
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
	eor	r4,r4,r8
	add	r8,sp,#4*(8)
	eor	r5,r5,r9
	str	r4,[r14],#16		r	eor	r6,r6,r10
	str	r5,[r14,#-12]
	eor	r7,r7,r11
	ldmia	r8,{r8,r9,r10,r11}	r	str	r6,[r14,#-8]
	add	r0,sp,#4*(16+8)
	str	r7,[r14,#-4]

	ldmia	r0,{r0,r1,r2,r3,r4,r5,r6,r7}	n
	add	r0,r0,r8	r	ldr	r8,[r12],#16		n	add	r1,r1,r9
	ldr	r9,[r12,#-12]
# ifdef	__thumb2__
	it	hi
# endif
	strhi	r10,[sp,#4*(16+10)]	r	add	r2,r2,r10
	ldr	r10,[r12,#-8]
# ifdef	__thumb2__
	it	hi
# endif
	strhi	r11,[sp,#4*(16+11)]	r	add	r3,r3,r11
	ldr	r11,[r12,#-4]
# ifdef	__ARMEB__
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
# endif
	eor	r0,r0,r8
	add	r8,sp,#4*(12)
	eor	r1,r1,r9
	str	r0,[r14],#16		r	eor	r2,r2,r10
	str	r1,[r14,#-12]
	eor	r3,r3,r11
	ldmia	r8,{r8,r9,r10,r11}	r	str	r2,[r14,#-8]
	str	r3,[r14,#-4]

	add	r4,r4,r8	r	add	r8,r8,#4		nnr	add	r5,r5,r9
	str	r8,[sp,#4*(12)]	nnr	ldr	r8,[r12],#16		n	add	r6,r6,r10
	add	r4,r4,#3		nr	ldr	r9,[r12,#-12]
	add	r7,r7,r11
	ldr	r10,[r12,#-8]
	ldr	r11,[r12,#-4]
# ifdef	__ARMEB__
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
	eor	r4,r4,r8
# ifdef	__thumb2__
	it	hi
# endif
	ldrhi	r8,[sp,#4*(32+2)]	rn	eor	r5,r5,r9
	eor	r6,r6,r10
	str	r4,[r14],#16		r	eor	r7,r7,r11
	str	r5,[r14,#-12]
	sub	r11,r8,#64*4	n	str	r6,[r14,#-8]
	str	r7,[r14,#-4]
	bhi	.Loop_neon_outer

	b	.Ldone_neon

.align	4
.Lbreak_neon:
	rnnnrnr	rrnrnnn	n
	str	r11, [sp,#4*(20+32+2)]	n	add	r11,sp,#4*(32+4)
	str	r12,   [sp,#4*(20+32+1)]	n	str	r14,   [sp,#4*(20+32+0)]	
	ldr	r12,[sp,#4*(16+10)]
	ldr	r14,[sp,#4*(16+11)]
	vldmia	r11,{d8,d9,d10,d11,d12,d13,d14,d15}			rrn	str	r12,[sp,#4*(20+16+10)]	r	str	r14,[sp,#4*(20+16+11)]	r
	ldr	r11, [sp,#4*(15)]
	ldr	r12,[sp,#4*(12)]			ldr	r10, [sp,#4*(13)]
	ldr	r14,[sp,#4*(14)]
	str	r11, [sp,#4*(20+16+15)]
	add	r11,sp,#4*(20)
	vst1.32	{q0,q1},[r11]!			add	sp,sp,#4*(20)			r	vst1.32	{q2,q3},[r11]
	mov	r11,#10
	b	.Loop				nrn
.align	4
.Ltail_neon:
	cmp	r11,#64*3
	bhs	.L192_or_more_neon
	cmp	r11,#64*2
	bhs	.L128_or_more_neon
	cmp	r11,#64*1
	bhs	.L64_or_more_neon

	add	r8,sp,#4*(8)
	vst1.8	{q0,q1},[sp]
	add	r10,sp,#4*(0)
	vst1.8	{q2,q3},[r8]
	b	.Loop_tail_neon

.align	4
.L64_or_more_neon:
	vld1.8	{q12,q13},[r12]!
	vld1.8	{q14,q15},[r12]!
	veor	q0,q0,q12
	veor	q1,q1,q13
	veor	q2,q2,q14
	veor	q3,q3,q15
	vst1.8	{q0,q1},[r14]!
	vst1.8	{q2,q3},[r14]!

	beq	.Ldone_neon

	add	r8,sp,#4*(8)
	vst1.8	{q4,q5},[sp]
	add	r10,sp,#4*(0)
	vst1.8	{q6,q7},[r8]
	sub	r11,r11,#64*1	n	b	.Loop_tail_neon

.align	4
.L128_or_more_neon:
	vld1.8	{q12,q13},[r12]!
	vld1.8	{q14,q15},[r12]!
	veor	q0,q0,q12
	veor	q1,q1,q13
	vld1.8	{q12,q13},[r12]!
	veor	q2,q2,q14
	veor	q3,q3,q15
	vld1.8	{q14,q15},[r12]!

	veor	q4,q4,q12
	veor	q5,q5,q13
	vst1.8	{q0,q1},[r14]!
	veor	q6,q6,q14
	vst1.8	{q2,q3},[r14]!
	veor	q7,q7,q15
	vst1.8	{q4,q5},[r14]!
	vst1.8	{q6,q7},[r14]!

	beq	.Ldone_neon

	add	r8,sp,#4*(8)
	vst1.8	{q8,q9},[sp]
	add	r10,sp,#4*(0)
	vst1.8	{q10,q11},[r8]
	sub	r11,r11,#64*2	n	b	.Loop_tail_neon

.align	4
.L192_or_more_neon:
	vld1.8	{q12,q13},[r12]!
	vld1.8	{q14,q15},[r12]!
	veor	q0,q0,q12
	veor	q1,q1,q13
	vld1.8	{q12,q13},[r12]!
	veor	q2,q2,q14
	veor	q3,q3,q15
	vld1.8	{q14,q15},[r12]!

	veor	q4,q4,q12
	veor	q5,q5,q13
	vld1.8	{q12,q13},[r12]!
	veor	q6,q6,q14
	vst1.8	{q0,q1},[r14]!
	veor	q7,q7,q15
	vld1.8	{q14,q15},[r12]!

	veor	q8,q8,q12
	vst1.8	{q2,q3},[r14]!
	veor	q9,q9,q13
	vst1.8	{q4,q5},[r14]!
	veor	q10,q10,q14
	vst1.8	{q6,q7},[r14]!
	veor	q11,q11,q15
	vst1.8	{q8,q9},[r14]!
	vst1.8	{q10,q11},[r14]!

	beq	.Ldone_neon

	ldmia	sp,{r8,r9,r10,r11}	r	add	r0,r0,r8	r	add	r8,sp,#4*(4)
	add	r1,r1,r9
	add	r2,r2,r10
	add	r3,r3,r11
	ldmia	r8,{r8,r9,r10,r11}	r
	add	r4,r4,r8	r	add	r8,sp,#4*(8)
	add	r5,r5,r9
	add	r6,r6,r10
	add	r7,r7,r11
	ldmia	r8,{r8,r9,r10,r11}	r# ifdef	__ARMEB__
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
	stmia	sp,{r0,r1,r2,r3,r4,r5,r6,r7}
	add	r0,sp,#4*(16+8)

	ldmia	r0,{r0,r1,r2,r3,r4,r5,r6,r7}	n
	add	r0,r0,r8	r	add	r8,sp,#4*(12)
	add	r1,r1,r9
	add	r2,r2,r10
	add	r3,r3,r11
	ldmia	r8,{r8,r9,r10,r11}	r
	add	r4,r4,r8	r	add	r8,sp,#4*(8)
	add	r5,r5,r9
	add	r4,r4,#3		nr	add	r6,r6,r10
	add	r7,r7,r11
	ldr	r11,[sp,#4*(32+2)]	rn# ifdef	__ARMEB__
	rev	r0,r0
	rev	r1,r1
	rev	r2,r2
	rev	r3,r3
	rev	r4,r4
	rev	r5,r5
	rev	r6,r6
	rev	r7,r7
# endif
	stmia	r8,{r0,r1,r2,r3,r4,r5,r6,r7}
	add	r10,sp,#4*(0)
	sub	r11,r11,#64*3	n
.Loop_tail_neon:
	ldrb	r8,[r10],#1	rrn	ldrb	r9,[r12],#1		rn	subs	r11,r11,#1
	eor	r8,r8,r9
	strb	r8,[r14],#1		r	bne	.Loop_tail_neon

.Ldone_neon:
	add	sp,sp,#4*(32+4)
	vldmia	sp,{d8,d9,d10,d11,d12,d13,d14,d15}
	add	sp,sp,#4*(16+3)
	ldmia	sp!,{r4,r5,r6,r7,r8,r9,r10,r11,pc}
.size	ChaCha20_ctr32_neon,.-ChaCha20_ctr32_neon
#endif
#endif  
