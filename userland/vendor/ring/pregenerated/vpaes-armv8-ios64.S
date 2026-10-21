#include <ring-core/asm_base.h>

#if !defined(OPENSSL_NO_ASM) && defined(OPENSSL_AARCH64) && defined(__APPLE__)
.section	__TEXT,__const

.align	7	
_vpaes_consts:
Lk_mc_forward:	
.quad	0x0407060500030201, 0x0C0F0E0D080B0A09
.quad	0x080B0A0904070605, 0x000302010C0F0E0D
.quad	0x0C0F0E0D080B0A09, 0x0407060500030201
.quad	0x000302010C0F0E0D, 0x080B0A0904070605
Lk_mc_backward:	
.quad	0x0605040702010003, 0x0E0D0C0F0A09080B
.quad	0x020100030E0D0C0F, 0x0A09080B06050407
.quad	0x0E0D0C0F0A09080B, 0x0605040702010003
.quad	0x0A09080B06050407, 0x020100030E0D0C0F
Lk_sr:	
.quad	0x0706050403020100, 0x0F0E0D0C0B0A0908
.quad	0x030E09040F0A0500, 0x0B06010C07020D08
.quad	0x0F060D040B020900, 0x070E050C030A0108
.quad	0x0B0E0104070A0D00, 0x0306090C0F020508

Lk_inv:	
.quad	0x0E05060F0D080180, 0x040703090A0B0C02
.quad	0x01040A060F0B0780, 0x030D0E0C02050809
Lk_ipt:	
.quad	0xC2B2E8985A2A7000, 0xCABAE09052227808
.quad	0x4C01307D317C4D00, 0xCD80B1FCB0FDCC81
Lk_sbo:	
.quad	0xD0D26D176FBDC700, 0x15AABF7AC502A878
.quad	0xCFE474A55FBB6A00, 0x8E1E90D1412B35FA
Lk_sb1:	
.quad	0x3618D415FAE22300, 0x3BF7CCC10D2ED9EF
.quad	0xB19BE18FCB503E00, 0xA5DF7A6E142AF544
Lk_sb2:	
.quad	0x69EB88400AE12900, 0xC2A163C8AB82234A
.quad	0xE27A93C60B712400, 0x5EB7E955BC982FCD

Lk_dksd:	
.quad	0xFEB91A5DA3E44700, 0x0740E3A45A1DBEF9
.quad	0x41C277F4B5368300, 0x5FDC69EAAB289D1E
Lk_dksb:	
.quad	0x9A4FCA1F8550D500, 0x03D653861CC94C99
.quad	0x115BEDA7B6FC4A00, 0xD993256F7E3482C8
Lk_dkse:	
.quad	0xD5031CCA1FC9D600, 0x53859A4C994F5086
.quad	0xA23196054FDC7BE8, 0xCD5EF96A20B31487
Lk_dks9:	
.quad	0xB6116FC87ED9A700, 0x4AED933482255BFC
.quad	0x4576516227143300, 0x8BB89FACE9DAFDCE

Lk_rcon:	
.quad	0x1F8391B9AF9DEEB6, 0x702A98084D7C7D81

Lk_opt:	
.quad	0xFF9F4929D6B66000, 0xF7974121DEBE6808
.quad	0x01EDBD5150BCEC00, 0xE10D5DB1B05C0CE0
Lk_deskew:	
.quad	0x07E4A34047A4E300, 0x1DFEB95A5DBEF91A
.quad	0x5F36B5DC83EA6900, 0x2841C2ABF49D1E77

.byte	86,101,99,116,111,114,32,80,101,114,109,117,116,97,116,105,111,110,32,65,69,83,32,102,111,114,32,65,82,77,118,56,44,32,77,105,107,101,32,72,97,109,98,117,114,103,32,40,83,116,97,110,102,111,114,100,32,85,110,105,118,101,114,115,105,116,121,41,0
.align	2

.align	6

.text

.align	4
_vpaes_encrypt_preheat:
	adrp	x10, Lk_inv@PAGE
	add	x10, x10, Lk_inv@PAGEOFF
	movi	v17.16b, #0x0f
	ld1	{v18.2d,v19.2d}, [x10],#32	
	ld1	{v20.2d,v21.2d,v22.2d,v23.2d}, [x10],#64	
	ld1	{v24.2d,v25.2d,v26.2d,v27.2d}, [x10]		
	ret

.align	4
_vpaes_encrypt_core:
	mov	x9, x2
	ldr	w8, [x2,#240]			
	adrp	x11, Lk_mc_forward@PAGE+16
	add	x11, x11, Lk_mc_forward@PAGEOFF+16

	ld1	{v16.2d}, [x9], #16		
	and	v1.16b, v7.16b, v17.16b		
	ushr	v0.16b, v7.16b, #4		
	tbl	v1.16b, {v20.16b}, v1.16b	

	tbl	v2.16b, {v21.16b}, v0.16b	
	eor	v0.16b, v1.16b, v16.16b		
	eor	v0.16b, v0.16b, v2.16b		
	b	Lenc_entry

.align	4
Lenc_loop:

	add	x10, x11, #0x40
	tbl	v4.16b, {v25.16b}, v2.16b		
	ld1	{v1.2d}, [x11], #16		
	tbl	v0.16b, {v24.16b}, v3.16b		
	eor	v4.16b, v4.16b, v16.16b		
	tbl	v5.16b,	{v27.16b}, v2.16b		
	eor	v0.16b, v0.16b, v4.16b		
	tbl	v2.16b, {v26.16b}, v3.16b		
	ld1	{v4.2d}, [x10]			
	tbl	v3.16b, {v0.16b}, v1.16b	
	eor	v2.16b, v2.16b, v5.16b		
	tbl	v0.16b, {v0.16b}, v4.16b	
	eor	v3.16b, v3.16b, v2.16b		
	tbl	v4.16b, {v3.16b}, v1.16b	
	eor	v0.16b, v0.16b, v3.16b		
	and	x11, x11, #~(1<<6)		
	eor	v0.16b, v0.16b, v4.16b		
	sub	w8, w8, #1			

Lenc_entry:

	and	v1.16b, v0.16b, v17.16b		
	ushr	v0.16b, v0.16b, #4		
	tbl	v5.16b, {v19.16b}, v1.16b	
	eor	v1.16b, v1.16b, v0.16b		
	tbl	v3.16b, {v18.16b}, v0.16b	
	tbl	v4.16b, {v18.16b}, v1.16b	
	eor	v3.16b, v3.16b, v5.16b		
	eor	v4.16b, v4.16b, v5.16b		
	tbl	v2.16b, {v18.16b}, v3.16b	
	tbl	v3.16b, {v18.16b}, v4.16b	
	eor	v2.16b, v2.16b, v1.16b		
	eor	v3.16b, v3.16b, v0.16b		
	ld1	{v16.2d}, [x9],#16		
	cbnz	w8, Lenc_loop

	add	x10, x11, #0x80

	tbl	v4.16b, {v22.16b}, v2.16b		
	ld1	{v1.2d}, [x10]			
	tbl	v0.16b, {v23.16b}, v3.16b		
	eor	v4.16b, v4.16b, v16.16b		
	eor	v0.16b, v0.16b, v4.16b		
	tbl	v0.16b, {v0.16b}, v1.16b	
	ret

.align	4
_vpaes_encrypt_2x:
	mov	x9, x2
	ldr	w8, [x2,#240]			
	adrp	x11, Lk_mc_forward@PAGE+16
	add	x11, x11, Lk_mc_forward@PAGEOFF+16

	ld1	{v16.2d}, [x9], #16		
	and	v1.16b,  v14.16b,  v17.16b	
	ushr	v0.16b,  v14.16b,  #4		
	and	v9.16b,  v15.16b,  v17.16b
	ushr	v8.16b,  v15.16b,  #4
	tbl	v1.16b,  {v20.16b}, v1.16b	
	tbl	v9.16b,  {v20.16b}, v9.16b

	tbl	v2.16b,  {v21.16b}, v0.16b	
	tbl	v10.16b, {v21.16b}, v8.16b
	eor	v0.16b,  v1.16b,   v16.16b	
	eor	v8.16b,  v9.16b,   v16.16b
	eor	v0.16b,  v0.16b,   v2.16b	
	eor	v8.16b,  v8.16b,   v10.16b
	b	Lenc_2x_entry

.align	4
Lenc_2x_loop:

	add	x10, x11, #0x40
	tbl	v4.16b,  {v25.16b}, v2.16b	
	tbl	v12.16b, {v25.16b}, v10.16b
	ld1	{v1.2d}, [x11], #16		
	tbl	v0.16b,  {v24.16b}, v3.16b	
	tbl	v8.16b,  {v24.16b}, v11.16b
	eor	v4.16b,  v4.16b,  v16.16b	
	eor	v12.16b, v12.16b, v16.16b
	tbl	v5.16b,	 {v27.16b}, v2.16b	
	tbl	v13.16b, {v27.16b}, v10.16b
	eor	v0.16b,  v0.16b,  v4.16b	
	eor	v8.16b,  v8.16b,  v12.16b
	tbl	v2.16b,  {v26.16b}, v3.16b	
	tbl	v10.16b, {v26.16b}, v11.16b
	ld1	{v4.2d}, [x10]			
	tbl	v3.16b,  {v0.16b}, v1.16b	
	tbl	v11.16b, {v8.16b}, v1.16b
	eor	v2.16b,  v2.16b,  v5.16b	
	eor	v10.16b, v10.16b, v13.16b
	tbl	v0.16b,  {v0.16b}, v4.16b	
	tbl	v8.16b,  {v8.16b}, v4.16b
	eor	v3.16b,  v3.16b,  v2.16b	
	eor	v11.16b, v11.16b, v10.16b
	tbl	v4.16b,  {v3.16b}, v1.16b	
	tbl	v12.16b, {v11.16b},v1.16b
	eor	v0.16b,  v0.16b,  v3.16b	
	eor	v8.16b,  v8.16b,  v11.16b
	and	x11, x11, #~(1<<6)		
	eor	v0.16b,  v0.16b,  v4.16b	
	eor	v8.16b,  v8.16b,  v12.16b
	sub	w8, w8, #1			

Lenc_2x_entry:

	and	v1.16b,  v0.16b, v17.16b	
	ushr	v0.16b,  v0.16b, #4		
	and	v9.16b,  v8.16b, v17.16b
	ushr	v8.16b,  v8.16b, #4
	tbl	v5.16b,  {v19.16b},v1.16b	
	tbl	v13.16b, {v19.16b},v9.16b
	eor	v1.16b,  v1.16b,  v0.16b	
	eor	v9.16b,  v9.16b,  v8.16b
	tbl	v3.16b,  {v18.16b},v0.16b	
	tbl	v11.16b, {v18.16b},v8.16b
	tbl	v4.16b,  {v18.16b},v1.16b	
	tbl	v12.16b, {v18.16b},v9.16b
	eor	v3.16b,  v3.16b,  v5.16b	
	eor	v11.16b, v11.16b, v13.16b
	eor	v4.16b,  v4.16b,  v5.16b	
	eor	v12.16b, v12.16b, v13.16b
	tbl	v2.16b,  {v18.16b},v3.16b	
	tbl	v10.16b, {v18.16b},v11.16b
	tbl	v3.16b,  {v18.16b},v4.16b	
	tbl	v11.16b, {v18.16b},v12.16b
	eor	v2.16b,  v2.16b,  v1.16b	
	eor	v10.16b, v10.16b, v9.16b
	eor	v3.16b,  v3.16b,  v0.16b	
	eor	v11.16b, v11.16b, v8.16b
	ld1	{v16.2d}, [x9],#16		
	cbnz	w8, Lenc_2x_loop

	add	x10, x11, #0x80

	tbl	v4.16b,  {v22.16b}, v2.16b	
	tbl	v12.16b, {v22.16b}, v10.16b
	ld1	{v1.2d}, [x10]			
	tbl	v0.16b,  {v23.16b}, v3.16b	
	tbl	v8.16b,  {v23.16b}, v11.16b
	eor	v4.16b,  v4.16b,  v16.16b	
	eor	v12.16b, v12.16b, v16.16b
	eor	v0.16b,  v0.16b,  v4.16b	
	eor	v8.16b,  v8.16b,  v12.16b
	tbl	v0.16b,  {v0.16b},v1.16b	
	tbl	v1.16b,  {v8.16b},v1.16b
	ret

.align	4
_vpaes_key_preheat:
	adrp	x10, Lk_inv@PAGE
	add	x10, x10, Lk_inv@PAGEOFF
	movi	v16.16b, #0x5b			
	adrp	x11, Lk_sb1@PAGE
	add	x11, x11, Lk_sb1@PAGEOFF
	movi	v17.16b, #0x0f			
	ld1	{v18.2d,v19.2d,v20.2d,v21.2d}, [x10]		
	adrp	x10, Lk_dksd@PAGE
	add	x10, x10, Lk_dksd@PAGEOFF
	ld1	{v22.2d,v23.2d}, [x11]		
	adrp	x11, Lk_mc_forward@PAGE
	add	x11, x11, Lk_mc_forward@PAGEOFF
	ld1	{v24.2d,v25.2d,v26.2d,v27.2d}, [x10],#64	
	ld1	{v28.2d,v29.2d,v30.2d,v31.2d}, [x10],#64	
	ld1	{v8.2d}, [x10]			
	ld1	{v9.2d}, [x11]			
	ret

.align	4
_vpaes_schedule_core:
	AARCH64_SIGN_LINK_REGISTER
	stp	x29, x30, [sp,#-16]!
	add	x29,sp,#0

	bl	_vpaes_key_preheat		

	ld1	{v0.16b}, [x0],#16		

	mov	v3.16b, v0.16b			
	bl	_vpaes_schedule_transform
	mov	v7.16b, v0.16b			

	adrp	x10, Lk_sr@PAGE		
	add	x10, x10, Lk_sr@PAGEOFF

	add	x8, x8, x10

	st1	{v0.2d}, [x2]			

	cmp	w1, #192			
	b.hi	Lschedule_256
	b.eq	Lschedule_192

Lschedule_128:
	mov	x0, #10			

Loop_schedule_128:
	sub	x0, x0, #1			
	bl	_vpaes_schedule_round
	cbz	x0, Lschedule_mangle_last
	bl	_vpaes_schedule_mangle		
	b	Loop_schedule_128

.align	4
Lschedule_192:
	sub	x0, x0, #8
	ld1	{v0.16b}, [x0]		
	bl	_vpaes_schedule_transform	
	mov	v6.16b, v0.16b			
	eor	v4.16b, v4.16b, v4.16b		
	ins	v6.d[0], v4.d[0]		
	mov	x0, #4			

Loop_schedule_192:
	sub	x0, x0, #1			
	bl	_vpaes_schedule_round
	ext	v0.16b, v6.16b, v0.16b, #8	
	bl	_vpaes_schedule_mangle		
	bl	_vpaes_schedule_192_smear
	bl	_vpaes_schedule_mangle		
	bl	_vpaes_schedule_round
	cbz	x0, Lschedule_mangle_last
	bl	_vpaes_schedule_mangle		
	bl	_vpaes_schedule_192_smear
	b	Loop_schedule_192

.align	4
Lschedule_256:
	ld1	{v0.16b}, [x0]		
	bl	_vpaes_schedule_transform	
	mov	x0, #7			

Loop_schedule_256:
	sub	x0, x0, #1			
	bl	_vpaes_schedule_mangle		
	mov	v6.16b, v0.16b			

	bl	_vpaes_schedule_round
	cbz	x0, Lschedule_mangle_last
	bl	_vpaes_schedule_mangle

	dup	v0.4s, v0.s[3]			
	movi	v4.16b, #0
	mov	v5.16b, v7.16b			
	mov	v7.16b, v6.16b			
	bl	_vpaes_schedule_low_round
	mov	v7.16b, v5.16b			

	b	Loop_schedule_256

.align	4
Lschedule_mangle_last:

	adrp	x11, Lk_deskew@PAGE	
	add	x11, x11, Lk_deskew@PAGEOFF

	cbnz	w3, Lschedule_mangle_last_dec

	ld1	{v1.2d}, [x8]			
	adrp	x11, Lk_opt@PAGE		
	add	x11, x11, Lk_opt@PAGEOFF
	add	x2, x2, #32			
	tbl	v0.16b, {v0.16b}, v1.16b	

Lschedule_mangle_last_dec:
	ld1	{v20.2d,v21.2d}, [x11]		
	sub	x2, x2, #16			
	eor	v0.16b, v0.16b, v16.16b		
	bl	_vpaes_schedule_transform	
	st1	{v0.2d}, [x2]			

	eor	v0.16b, v0.16b, v0.16b		
	eor	v1.16b, v1.16b, v1.16b		
	eor	v2.16b, v2.16b, v2.16b		
	eor	v3.16b, v3.16b, v3.16b		
	eor	v4.16b, v4.16b, v4.16b		
	eor	v5.16b, v5.16b, v5.16b		
	eor	v6.16b, v6.16b, v6.16b		
	eor	v7.16b, v7.16b, v7.16b		
	ldp	x29, x30, [sp],#16
	AARCH64_VALIDATE_LINK_REGISTER
	ret

.align	4
_vpaes_schedule_192_smear:
	movi	v1.16b, #0
	dup	v0.4s, v7.s[3]
	ins	v1.s[3], v6.s[2]	
	ins	v0.s[0], v7.s[2]	
	eor	v6.16b, v6.16b, v1.16b	
	eor	v1.16b, v1.16b, v1.16b	
	eor	v6.16b, v6.16b, v0.16b	
	mov	v0.16b, v6.16b		
	ins	v6.d[0], v1.d[0]	
	ret

.align	4
_vpaes_schedule_round:

	movi	v4.16b, #0			
	ext	v1.16b, v8.16b, v4.16b, #15	
	ext	v8.16b, v8.16b, v8.16b, #15	
	eor	v7.16b, v7.16b, v1.16b		

	dup	v0.4s, v0.s[3]			
	ext	v0.16b, v0.16b, v0.16b, #1	

_vpaes_schedule_low_round:

	ext	v1.16b, v4.16b, v7.16b, #12	
	eor	v7.16b, v7.16b, v1.16b		
	ext	v4.16b, v4.16b, v7.16b, #8	

	and	v1.16b, v0.16b, v17.16b		
	ushr	v0.16b, v0.16b, #4		
	eor	v7.16b, v7.16b, v4.16b		
	tbl	v2.16b, {v19.16b}, v1.16b	
	eor	v1.16b, v1.16b, v0.16b		
	tbl	v3.16b, {v18.16b}, v0.16b	
	eor	v3.16b, v3.16b, v2.16b		
	tbl	v4.16b, {v18.16b}, v1.16b	
	eor	v7.16b, v7.16b, v16.16b		
	tbl	v3.16b, {v18.16b}, v3.16b	
	eor	v4.16b, v4.16b, v2.16b		
	tbl	v2.16b, {v18.16b}, v4.16b	
	eor	v3.16b, v3.16b, v1.16b		
	eor	v2.16b, v2.16b, v0.16b		
	tbl	v4.16b, {v23.16b}, v3.16b	
	tbl	v1.16b, {v22.16b}, v2.16b	
	eor	v1.16b, v1.16b, v4.16b		

	eor	v0.16b, v1.16b, v7.16b		
	eor	v7.16b, v1.16b, v7.16b		
	ret

.align	4
_vpaes_schedule_transform:
	and	v1.16b, v0.16b, v17.16b		
	ushr	v0.16b, v0.16b, #4		

	tbl	v2.16b, {v20.16b}, v1.16b	

	tbl	v0.16b, {v21.16b}, v0.16b	
	eor	v0.16b, v0.16b, v2.16b		
	ret

.align	4
_vpaes_schedule_mangle:
	mov	v4.16b, v0.16b			

	eor	v4.16b, v0.16b, v16.16b		
	add	x2, x2, #16			
	tbl	v4.16b, {v4.16b}, v9.16b	
	tbl	v1.16b, {v4.16b}, v9.16b	
	tbl	v3.16b, {v1.16b}, v9.16b	
	eor	v4.16b, v4.16b, v1.16b		
	ld1	{v1.2d}, [x8]			
	eor	v3.16b, v3.16b, v4.16b		

Lschedule_mangle_both:
	tbl	v3.16b, {v3.16b}, v1.16b	
	add	x8, x8, #48			
	and	x8, x8, #~(1<<6)		
	st1	{v3.2d}, [x2]			
	ret

.globl	_vpaes_set_encrypt_key
.private_extern	_vpaes_set_encrypt_key

.align	4
_vpaes_set_encrypt_key:
	AARCH64_SIGN_LINK_REGISTER
	stp	x29,x30,[sp,#-16]!
	add	x29,sp,#0
	stp	d8,d9,[sp,#-16]!	

	lsr	w9, w1, #5		
	add	w9, w9, #5		
	str	w9, [x2,#240]		

	mov	w3, #0		
	mov	x8, #0x30		
	bl	_vpaes_schedule_core
	eor	x0, x0, x0

	ldp	d8,d9,[sp],#16
	ldp	x29,x30,[sp],#16
	AARCH64_VALIDATE_LINK_REGISTER
	ret

.globl	_vpaes_ctr32_encrypt_blocks
.private_extern	_vpaes_ctr32_encrypt_blocks

.align	4
_vpaes_ctr32_encrypt_blocks:
	AARCH64_SIGN_LINK_REGISTER
	stp	x29,x30,[sp,#-16]!
	add	x29,sp,#0
	stp	d8,d9,[sp,#-16]!	
	stp	d10,d11,[sp,#-16]!
	stp	d12,d13,[sp,#-16]!
	stp	d14,d15,[sp,#-16]!

	cbz	x2, Lctr32_done

	mov	x17, x2
	mov	x2,  x3

	ldr	w6, [x4, #12]
	ld1	{v7.16b}, [x4]

	bl	_vpaes_encrypt_preheat
	tst	x17, #1
	rev	w6, w6		
	b.eq	Lctr32_prep_loop

	ld1	{v6.16b}, [x0], #16	
	bl	_vpaes_encrypt_core
	eor	v0.16b, v0.16b, v6.16b	
	st1	{v0.16b}, [x1], #16
	subs	x17, x17, #1

	add	w6, w6, #1
	rev	w7, w6
	mov	v7.s[3], w7
	b.ls	Lctr32_done

Lctr32_prep_loop:

	mov	v15.16b, v7.16b
	mov	v14.16b, v7.16b
	add	w6, w6, #1
	rev	w7, w6
	mov	v15.s[3], w7

Lctr32_loop:
	ld1	{v6.16b,v7.16b}, [x0], #32	
	bl	_vpaes_encrypt_2x
	eor	v0.16b, v0.16b, v6.16b		
	eor	v1.16b, v1.16b, v7.16b		
	st1	{v0.16b,v1.16b}, [x1], #32
	subs	x17, x17, #2

	add	w7, w6, #1
	add	w6, w6, #2
	rev	w7, w7
	mov	v14.s[3], w7
	rev	w7, w6
	mov	v15.s[3], w7
	b.hi	Lctr32_loop

Lctr32_done:
	ldp	d14,d15,[sp],#16
	ldp	d12,d13,[sp],#16
	ldp	d10,d11,[sp],#16
	ldp	d8,d9,[sp],#16
	ldp	x29,x30,[sp],#16
	AARCH64_VALIDATE_LINK_REGISTER
	ret

#endif  
