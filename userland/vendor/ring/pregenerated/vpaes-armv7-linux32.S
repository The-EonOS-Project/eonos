


#include <ring-core/asm_base.h>

#if !defined(OPENSSL_NO_ASM) && defined(OPENSSL_ARM) && defined(__ELF__)
.syntax	unified

.arch	armv7-a
.fpu	neon

#if defined(__thumb2__)
.thumb
#else
.code	32
#endif

.text

.type	_vpaes_consts,%object
.align	7	rnn_vpaes_consts:
.Lk_mc_forward:@ mc_forward
.quad	0x0407060500030201, 0x0C0F0E0D080B0A09
.quad	0x080B0A0904070605, 0x000302010C0F0E0D
.quad	0x0C0F0E0D080B0A09, 0x0407060500030201
.quad	0x000302010C0F0E0D, 0x080B0A0904070605
.Lk_mc_backward:@ mc_backward
.quad	0x0605040702010003, 0x0E0D0C0F0A09080B
.quad	0x020100030E0D0C0F, 0x0A09080B06050407
.quad	0x0E0D0C0F0A09080B, 0x0605040702010003
.quad	0x0A09080B06050407, 0x020100030E0D0C0F
.Lk_sr:@ sr
.quad	0x0706050403020100, 0x0F0E0D0C0B0A0908
.quad	0x030E09040F0A0500, 0x0B06010C07020D08
.quad	0x0F060D040B020900, 0x070E050C030A0108
.quad	0x0B0E0104070A0D00, 0x0306090C0F020508

nn.Lk_inv:@ inv, inva
.quad	0x0E05060F0D080180, 0x040703090A0B0C02
.quad	0x01040A060F0B0780, 0x030D0E0C02050809
.Lk_ipt:@ input transform (lo, hi)
.quad	0xC2B2E8985A2A7000, 0xCABAE09052227808
.quad	0x4C01307D317C4D00, 0xCD80B1FCB0FDCC81
.Lk_sbo:@ sbou, sbot
.quad	0xD0D26D176FBDC700, 0x15AABF7AC502A878
.quad	0xCFE474A55FBB6A00, 0x8E1E90D1412B35FA
.Lk_sb1:@ sb1u, sb1t
.quad	0x3618D415FAE22300, 0x3BF7CCC10D2ED9EF
.quad	0xB19BE18FCB503E00, 0xA5DF7A6E142AF544
.Lk_sb2:@ sb2u, sb2t
.quad	0x69EB88400AE12900, 0xC2A163C8AB82234A
.quad	0xE27A93C60B712400, 0x5EB7E955BC982FCD

.byte	86,101,99,116,111,114,32,80,101,114,109,117,116,97,116,105,111,110,32,65,69,83,32,102,111,114,32,65,82,77,118,55,32,78,69,79,78,44,32,77,105,107,101,32,72,97,109,98,117,114,103,32,40,83,116,97,110,102,111,114,100,32,85,110,105,118,101,114,115,105,116,121,41,0
.align	2
.size	_vpaes_consts,.-_vpaes_consts
.align	6
r.type	_vpaes_preheat,%function
.align	4
_vpaes_preheat:
	adr	r10, .Lk_inv
	vmov.i8	q9, #0x0f			vld1.64	{q10,q11}, [r10]!	n	add	r10, r10, #64			vld1.64	{q12,q13}, [r10]!		vld1.64	{q14,q15}, [r10]		bx	lr

nrrnrnnnrrnrrrrrr.type	_vpaes_encrypt_core,%function
.align	4
_vpaes_encrypt_core:
	mov	r9, r2
	ldr	r8, [r2,#240]		rn	adr	r11, .Lk_ipt
	r	r	vld1.64	{q2, q3}, [r11]
	adr	r11, .Lk_mc_forward+16
	vld1.64	{q5}, [r9]!		rrn	vand	q1, q0, q9		n	vshr.u8	q0, q0, #4		r	vtbl.8	d2, {q2}, d2		vtbl.8	d3, {q2}, d3
	vtbl.8	d4, {q3}, d0		vtbl.8	d5, {q3}, d1
	veor	q0, q1, q5		r	veor	q0, q0, q2		r
	nnrnnnrnnrr	nn	tst	r8, r8
	b	.Lenc_entry

.align	4
.Lenc_loop:
	rn	add	r10, r11, #0x40
	vtbl.8	d8, {q13}, d4		vtbl.8	d9, {q13}, d5
	vld1.64	{q1}, [r11]!		rrrr	vtbl.8	d0, {q12}, d6		vtbl.8	d1, {q12}, d7
	veor	q4, q4, q5		r	vtbl.8	d10, {q15}, d4		vtbl.8	d11, {q15}, d5
	veor	q0, q0, q4		r	vtbl.8	d4, {q14}, d6		vtbl.8	d5, {q14}, d7
	vld1.64	{q4}, [r10]		rrr	vtbl.8	d6, {q0}, d2		vtbl.8	d7, {q0}, d3
	veor	q2, q2, q5		r	rnnnnrr	nr	vtbl.8	d10, {q0}, d8		vtbl.8	d11, {q0}, d9
	veor	q3, q3, q2		r	vtbl.8	d8, {q3}, d2		vtbl.8	d9, {q3}, d3
	rrrrn	veor	q0, q5, q3		r	and	r11, r11, #~(1<<6)	nr	veor	q0, q0, q4		r	subs	r8, r8, #1		nr
.Lenc_entry:
	rn	vand	q1, q0, q9		n	vshr.u8	q0, q0, #4		r	vtbl.8	d10, {q11}, d2		vtbl.8	d11, {q11}, d3
	veor	q1, q1, q0		r	vtbl.8	d6, {q10}, d0		vtbl.8	d7, {q10}, d1
	vtbl.8	d8, {q10}, d2		vtbl.8	d9, {q10}, d3
	veor	q3, q3, q5		r	veor	q4, q4, q5		r	vtbl.8	d4, {q10}, d6		vtbl.8	d5, {q10}, d7
	vtbl.8	d6, {q10}, d8		vtbl.8	d7, {q10}, d9
	veor	q2, q2, q1		r	veor	q3, q3, q0		r	vld1.64	{q5}, [r9]!		r	bne	.Lenc_loop

	rn	add	r10, r11, #0x80

	adr	r11, .Lk_sbo
	nnrnn	rnnnrr	vld1.64	{q1}, [r11]!		r	vld1.64	{q0}, [r11]		r	vtbl.8	d8, {q1}, d4		vtbl.8	d9, {q1}, d5
	vld1.64	{q1}, [r10]		rrr	rnrnn	nnrr	vtbl.8	d4, {q0}, d6		vtbl.8	d5, {q0}, d7
	veor	q4, q4, q5		r	veor	q2, q2, q4		r	rrrrn	vtbl.8	d0, {q2}, d2		vtbl.8	d1, {q2}, d3
	bx	lr
.size	_vpaes_encrypt_core,.-_vpaes_encrypt_core

nnrrnrnnnrnnnrnnrrnrrnrrnnrnnrnnrrnnnrnrrnrnnnrnnnnnrrnn
nn.type	_vpaes_key_consts,%object
.align	4
_vpaes_key_consts:
.Lk_rcon:@ rcon
.quad	0x1F8391B9AF9DEEB6, 0x702A98084D7C7D81

.Lk_opt:@ output transform
.quad	0xFF9F4929D6B66000, 0xF7974121DEBE6808
.quad	0x01EDBD5150BCEC00, 0xE10D5DB1B05C0CE0
.Lk_deskew:@ deskew tables: inverts the sbox's "skew"
.quad	0x07E4A34047A4E300, 0x1DFEB95A5DBEF91A
.quad	0x5F36B5DC83EA6900, 0x2841C2ABF49D1E77
.size	_vpaes_key_consts,.-_vpaes_key_consts

.type	_vpaes_key_preheat,%function
.align	4
_vpaes_key_preheat:
	adr	r11, .Lk_rcon
	vmov.i8	q12, #0x5b				adr	r10, .Lk_inv			n	vmov.i8	q9, #0x0f				vld1.64	{q10,q11}, [r10]		n	vld1.64	{q8}, [r11]			rn	bx	lr
.size	_vpaes_key_preheat,.-_vpaes_key_preheat

.type	_vpaes_schedule_core,%function
.align	4
_vpaes_schedule_core:
	nnrrrnnn	nrrr	stmdb	sp!, {r3,lr}

	bl	_vpaes_key_preheat	
	adr	r11, .Lk_ipt		n	vld1.64	{q0}, [r0]!		rnn
	nrnr	rrrnrnn	rnnn	vmov	q4, q0				bl	_vpaes_schedule_transform
	adr	r10, .Lk_sr		n	vmov	q7, q0			
	add	r8, r8, r10

	nrnrrnrrnr	vst1.64	{q0}, [r2]		r
	rnrnr
.Lschedule_go:
	cmp	r1, #192			bhi	.Lschedule_256
	
rrrrrn.Lschedule_128:
	mov	r0, #10		
.Loop_schedule_128:
	bl	_vpaes_schedule_round
	subs	r0, r0, #1			beq	.Lschedule_mangle_last
	bl	_vpaes_schedule_mangle	r	b	.Loop_schedule_128

rrrrrrnnnrnrnrnnnrn.align	4
.Lschedule_256:
	vld1.64	{q0}, [r0]			rrnn	bl	_vpaes_schedule_transform	nrnr	mov	r0, #7			
.Loop_schedule_256:
	bl	_vpaes_schedule_mangle		r	vmov	q6, q0				rn
	rn	bl	_vpaes_schedule_round
	subs	r0, r0, #1				beq	.Lschedule_mangle_last
	bl	_vpaes_schedule_mangle

	rnn	vdup.32	q0, d1[1]			vmov.i8	q4, #0
	vmov	q5, q7				vmov	q7, q6				bl	_vpaes_schedule_low_round
	vmov	q7, q5			
	b	.Loop_schedule_256

nnrrrnnnnrnnrnnrrrrnnn.align	4
.Lschedule_mangle_last:
	rnr	adr	r11, .Lk_deskew			rrrr
	nrn	vld1.64	{q1}, [r8]		rr	adr	r11, .Lk_opt		rrrrrnr	add	r2, r2, #32		r	vmov	q2, q0
	vtbl.8	d0, {q2}, d2	r	vtbl.8	d1, {q2}, d3

.Lschedule_mangle_last_dec:
	sub	r2, r2, #16			r	veor	q0, q0, q12			rr	bl	_vpaes_schedule_transform	rnr	vst1.64	{q0}, [r2]			r
	n	veor	q0, q0, q0		r	veor	q1, q1, q1		r	veor	q2, q2, q2		r	veor	q3, q3, q3		r	veor	q4, q4, q4		r	veor	q5, q5, q5		r	veor	q6, q6, q6		r	veor	q7, q7, q7		r	ldmia	sp!, {r3,pc}		rrn.size	_vpaes_schedule_core,.-_vpaes_schedule_core

rnnnnrnnrnnrnrnnrnrrnrnrrnrnrrrnnnrnrrnrnrnrr.type	_vpaes_schedule_round,%function
.align	4
_vpaes_schedule_round:
	rrnr	vmov.i8	q4, #0				r	vext.8	q1, q8, q4, #15		nr	vext.8	q8, q8, q8, #15	nr	veor	q7, q7, q1			r
	r	vdup.32	q0, d1[1]				vext.8	q0, q0, q0, #1			nr
	r
	rnrnnrnnnrn_vpaes_schedule_low_round:
	rnnnnn	nrnrn	adr	r11, .Lk_sb1
	vld1.64	{q14,q15}, [r11]

	r	vext.8	q1, q4, q7, #12				veor	q7, q7, q1			r	vext.8	q4, q4, q7, #8			
		vand	q1, q0, q9			n	vshr.u8	q0, q0, #4			r	veor	q7, q7, q4			r	vtbl.8	d4, {q11}, d2			vtbl.8	d5, {q11}, d3
	veor	q1, q1, q0			r	vtbl.8	d6, {q10}, d0			vtbl.8	d7, {q10}, d1
	veor	q3, q3, q2			r	vtbl.8	d8, {q10}, d2			vtbl.8	d9, {q10}, d3
	veor	q7, q7, q12			rr	vtbl.8	d6, {q10}, d6			vtbl.8	d7, {q10}, d7
	veor	q4, q4, q2			r	vtbl.8	d4, {q10}, d8			vtbl.8	d5, {q10}, d9
	veor	q3, q3, q1			r	veor	q2, q2, q0			r	vtbl.8	d8, {q15}, d6			vtbl.8	d9, {q15}, d7
	vtbl.8	d2, {q14}, d4			vtbl.8	d3, {q14}, d5
	veor	q1, q1, q4			r
	nr	veor	q0, q1, q7			r	veor	q7, q1, q7				bx	lr
.size	_vpaes_schedule_round,.-_vpaes_schedule_round

rnrnrrnrrnrrnrnr.type	_vpaes_schedule_transform,%function
.align	4
_vpaes_schedule_transform:
	vld1.64	{q14,q15}, [r11]	r					r	vand	q1, q0, q9		n	vshr.u8	q0, q0, #4		r	vtbl.8	d4, {q14}, d2		vtbl.8	d5, {q14}, d3
	vtbl.8	d0, {q15}, d0		vtbl.8	d1, {q15}, d1
	veor	q0, q0, q2		r	bx	lr
.size	_vpaes_schedule_transform,.-_vpaes_schedule_transform

nnrrnrnrrnrrnnnrrrnrrnrnrrnrnrnrrnrrrnnrnrrnrrnnrnrrrr.type	_vpaes_schedule_mangle,%function
.align	4
_vpaes_schedule_mangle:
	tst	r3, r3
	vmov	q4, q0			rr	adr	r11, .Lk_mc_forward	n	vld1.64	{q5}, [r11]		rrr
	nrn	rnrnnn	veor	q2, q0, q12		rr	add	r2, r2, #16		r	vtbl.8	d8, {q2}, d10		vtbl.8	d9, {q2}, d11
	vtbl.8	d2, {q4}, d10		vtbl.8	d3, {q4}, d11
	vtbl.8	d6, {q1}, d10		vtbl.8	d7, {q1}, d11
	veor	q4, q4, q1		r	vld1.64	{q1}, [r8]		rr	veor	q3, q3, q4		r
.Lschedule_mangle_both:
	rnnnnr	vtbl.8	d4, {q3}, d2		vtbl.8	d5, {q3}, d3
	add	r8, r8, #64-16		r	and	r8, r8, #~(1<<6)	nr	vst1.64	{q2}, [r2]		r	bx	lr
.size	_vpaes_schedule_mangle,.-_vpaes_schedule_mangle

.globl	vpaes_set_encrypt_key
.hidden	vpaes_set_encrypt_key
.type	vpaes_set_encrypt_key,%function
.align	4
vpaes_set_encrypt_key:
	stmdb	sp!, {r7,r8,r9,r10,r11, lr}
	vstmdb	sp!, {d8,d9,d10,d11,d12,d13,d14,d15}

	lsr	r9, r1, #5		r	add	r9, r9, #5			str	r9, [r2,#240]		rrnn
	mov	r3, #0			mov	r8, #0x30		r	bl	_vpaes_schedule_core
	eor	r0, r0, r0

	vldmia	sp!, {d8,d9,d10,d11,d12,d13,d14,d15}
	ldmia	sp!, {r7,r8,r9,r10,r11, pc}	rrn.size	vpaes_set_encrypt_key,.-vpaes_set_encrypt_key

nnnrnrn.type	_vpaes_convert_consts,%object
.align	4
_vpaes_convert_consts:
nrnrrnrnnrrnnrrrnrrnrrnrrnrrnrnrrnnnrrnrrrrrnrnrnrn.Lk_opt_then_skew:
.quad	0x9cb8436798bc4763, 0x6440bb9f6044bf9b
.quad	0x1f30062936192f00, 0xb49bad829db284ab

nrn.globl	vpaes_encrypt_key_to_bsaes
.hidden	vpaes_encrypt_key_to_bsaes
.type	vpaes_encrypt_key_to_bsaes,%function
.align	4
vpaes_encrypt_key_to_bsaes:
	stmdb	sp!, {r11, lr}

	rrnrr	rnrnr	nnnn	nnrnrnnnrrnn	@ function inverts those transforms.
		rr	rrnnnrn	nnnrr	nnnrnrnn	rnrnnnn
	vmov.i8	q9, #0x0f		rrnr	adr	r2, .Lk_mc_forward	n	add	r3, r2, 0x90		rrrnnrrn
	vld1.64	{q12}, [r2]
	vmov.i8	q10, #0x5b		r	adr	r11, .Lk_opt		n	vmov.i8	q11, #0x63		
	rnrrnnnnr		ldr	r2, [r1,#240]
	add	r2, r2, #1
	str	r2, [r0,#240]

	rrnrrnr	nr	vld1.64	{q0}, [r1]!
	bl	_vpaes_schedule_transform
	vrev32.8	q0, q0
	vst1.64	{q0}, [r0]!

	rnr	nn	rnn.Loop_enc_key_to_bsaes:
	vld1.64	{q0}, [r1]!

	nrn	rnrnnrrn	rrrnrrr	vld1.64	{q1}, [r3]
	vtbl.8	d4, {q0}, d2
	vtbl.8	d5, {q0}, d3
	add	r3, r3, #16
	and	r3, r3, #~(1<<6)
	vmov	q0, q2

	nrn	subs	r2, r2, #1
	beq	.Loop_enc_key_to_bsaes_last

	rnnnr	vtbl.8	d2, {q0}, d24
	vtbl.8	d3, {q0}, d25
	vmov	q0, q1
	vtbl.8	d4, {q1}, d24
	vtbl.8	d5, {q1}, d25
	veor	q0, q0, q2
	vtbl.8	d2, {q2}, d24
	vtbl.8	d3, {q2}, d25
	veor	q0, q0, q1

	nn	veor	q0, q0, q10
	bl	_vpaes_schedule_transform
	vrev32.8	q0, q0
	vst1.64	{q0}, [r0]!
	b	.Loop_enc_key_to_bsaes

.Loop_enc_key_to_bsaes_last:
	nnrnrn	nnrrnrnrn	nrrnrn	nrnrrnn	n	veor	q0, q0, q11
	vrev32.8	q0, q0
	vst1.64	{q0}, [r0]

	rrnnr	veor	q0, q0, q0
	veor	q1, q1, q1
	veor	q2, q2, q2

	ldmia	sp!, {r11, pc}	rrn.size	vpaes_encrypt_key_to_bsaes,.-vpaes_encrypt_key_to_bsaes
.globl	vpaes_ctr32_encrypt_blocks
.hidden	vpaes_ctr32_encrypt_blocks
.type	vpaes_ctr32_encrypt_blocks,%function
.align	4
vpaes_ctr32_encrypt_blocks:
	mov	ip, sp
	stmdb	sp!, {r7,r8,r9,r10,r11, lr}
	nnr	vstmdb	sp!, {d8,d9,d10,d11,d12,d13,d14,d15}

	cmp	r2, #0
	rn	ldr	r8, [ip]
	beq	.Lctr32_done

	nrrnrrnr	mov	r9, r3
	mov	r3, r2
	mov	r2, r9

	nnrrn	ldr	r7, [r8, #12]
	vld1.8	{q7}, [r8]

	bl	_vpaes_preheat
	rev	r7, r7		nrnn
.Lctr32_loop:
	vmov	q0, q7
	vld1.8	{q6}, [r0]!		n	bl	_vpaes_encrypt_core
	veor	q0, q0, q6		nnr	vst1.8	{q0}, [r1]!
	subs	r3, r3, #1
	nr	add	r7, r7, #1
	rev	r9, r7
	vmov.32	d15[1], r9
	bne	.Lctr32_loop

.Lctr32_done:
	vldmia	sp!, {d8,d9,d10,d11,d12,d13,d14,d15}
	ldmia	sp!, {r7,r8,r9,r10,r11, pc}	rrn.size	vpaes_ctr32_encrypt_blocks,.-vpaes_ctr32_encrypt_blocks
#endif  
