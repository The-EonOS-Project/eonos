


#include <ring-core/asm_base.h>

#if !defined(OPENSSL_NO_ASM) && defined(OPENSSL_AARCH64) && defined(__ELF__)
#if __ARM_MAX_ARCH__>=7
.text
.arch	armv8-a+crypto
.globl	gcm_init_clmul
.hidden	gcm_init_clmul
.type	gcm_init_clmul,%function
.align	4
gcm_init_clmul:
	AARCH64_VALID_CALL_TARGET
	ld1	{v17.2d},[x1]		
	movi	v19.16b,#0xe1
	shl	v19.2d,v19.2d,#57		
	ext	v3.16b,v17.16b,v17.16b,#8
	ushr	v18.2d,v19.2d,#63
	dup	v17.4s,v17.s[1]
	ext	v16.16b,v18.16b,v19.16b,#8		
	ushr	v18.2d,v3.2d,#63
	sshr	v17.4s,v17.4s,#31		
	and	v18.16b,v18.16b,v16.16b
	shl	v3.2d,v3.2d,#1
	ext	v18.16b,v18.16b,v18.16b,#8
	and	v16.16b,v16.16b,v17.16b
	orr	v3.16b,v3.16b,v18.16b		
	eor	v20.16b,v3.16b,v16.16b		
	st1	{v20.2d},[x0],#16		

	
	ext	v16.16b,v20.16b,v20.16b,#8		
	pmull	v0.1q,v20.1d,v20.1d
	eor	v16.16b,v16.16b,v20.16b
	pmull2	v2.1q,v20.2d,v20.2d
	pmull	v1.1q,v16.1d,v16.1d

	ext	v17.16b,v0.16b,v2.16b,#8		
	eor	v18.16b,v0.16b,v2.16b
	eor	v1.16b,v1.16b,v17.16b
	eor	v1.16b,v1.16b,v18.16b
	pmull	v18.1q,v0.1d,v19.1d		

	ins	v2.d[0],v1.d[1]
	ins	v1.d[1],v0.d[0]
	eor	v0.16b,v1.16b,v18.16b

	ext	v18.16b,v0.16b,v0.16b,#8		
	pmull	v0.1q,v0.1d,v19.1d
	eor	v18.16b,v18.16b,v2.16b
	eor	v22.16b,v0.16b,v18.16b

	ext	v17.16b,v22.16b,v22.16b,#8		
	eor	v17.16b,v17.16b,v22.16b
	ext	v21.16b,v16.16b,v17.16b,#8		
	st1	{v21.2d,v22.2d},[x0],#32	
	
	pmull	v0.1q,v20.1d, v22.1d
	pmull	v5.1q,v22.1d,v22.1d
	pmull2	v2.1q,v20.2d, v22.2d
	pmull2	v7.1q,v22.2d,v22.2d
	pmull	v1.1q,v16.1d,v17.1d
	pmull	v6.1q,v17.1d,v17.1d

	ext	v16.16b,v0.16b,v2.16b,#8		
	ext	v17.16b,v5.16b,v7.16b,#8
	eor	v18.16b,v0.16b,v2.16b
	eor	v1.16b,v1.16b,v16.16b
	eor	v4.16b,v5.16b,v7.16b
	eor	v6.16b,v6.16b,v17.16b
	eor	v1.16b,v1.16b,v18.16b
	pmull	v18.1q,v0.1d,v19.1d		
	eor	v6.16b,v6.16b,v4.16b
	pmull	v4.1q,v5.1d,v19.1d

	ins	v2.d[0],v1.d[1]
	ins	v7.d[0],v6.d[1]
	ins	v1.d[1],v0.d[0]
	ins	v6.d[1],v5.d[0]
	eor	v0.16b,v1.16b,v18.16b
	eor	v5.16b,v6.16b,v4.16b

	ext	v18.16b,v0.16b,v0.16b,#8		
	ext	v4.16b,v5.16b,v5.16b,#8
	pmull	v0.1q,v0.1d,v19.1d
	pmull	v5.1q,v5.1d,v19.1d
	eor	v18.16b,v18.16b,v2.16b
	eor	v4.16b,v4.16b,v7.16b
	eor	v20.16b, v0.16b,v18.16b		
	eor	v22.16b,v5.16b,v4.16b		

	ext	v16.16b,v20.16b, v20.16b,#8		
	ext	v17.16b,v22.16b,v22.16b,#8
	eor	v16.16b,v16.16b,v20.16b
	eor	v17.16b,v17.16b,v22.16b
	ext	v21.16b,v16.16b,v17.16b,#8		
	st1	{v20.2d,v21.2d,v22.2d},[x0]		
	ret
.size	gcm_init_clmul,.-gcm_init_clmul
.globl	gcm_gmult_clmul
.hidden	gcm_gmult_clmul
.type	gcm_gmult_clmul,%function
.align	4
gcm_gmult_clmul:
	AARCH64_VALID_CALL_TARGET
	ld1	{v17.2d},[x0]		
	movi	v19.16b,#0xe1
	ld1	{v20.2d,v21.2d},[x1]	
	shl	v19.2d,v19.2d,#57
#ifndef __AARCH64EB__
	rev64	v17.16b,v17.16b
#endif
	ext	v3.16b,v17.16b,v17.16b,#8

	pmull	v0.1q,v20.1d,v3.1d		
	eor	v17.16b,v17.16b,v3.16b		
	pmull2	v2.1q,v20.2d,v3.2d		
	pmull	v1.1q,v21.1d,v17.1d		

	ext	v17.16b,v0.16b,v2.16b,#8		
	eor	v18.16b,v0.16b,v2.16b
	eor	v1.16b,v1.16b,v17.16b
	eor	v1.16b,v1.16b,v18.16b
	pmull	v18.1q,v0.1d,v19.1d		

	ins	v2.d[0],v1.d[1]
	ins	v1.d[1],v0.d[0]
	eor	v0.16b,v1.16b,v18.16b

	ext	v18.16b,v0.16b,v0.16b,#8		
	pmull	v0.1q,v0.1d,v19.1d
	eor	v18.16b,v18.16b,v2.16b
	eor	v0.16b,v0.16b,v18.16b

#ifndef __AARCH64EB__
	rev64	v0.16b,v0.16b
#endif
	ext	v0.16b,v0.16b,v0.16b,#8
	st1	{v0.2d},[x0]		

	ret
.size	gcm_gmult_clmul,.-gcm_gmult_clmul
.byte	71,72,65,83,72,32,102,111,114,32,65,82,77,118,56,44,32,67,82,89,80,84,79,71,65,77,83,32,98,121,32,60,97,112,112,114,111,64,111,112,101,110,115,115,108,46,111,114,103,62,0
.align	2
.align	2
#endif
#endif  
