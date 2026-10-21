#ifndef OPENSSL_HEADER_AES_H
#define OPENSSL_HEADER_AES_H

#include <ring-core/base.h>

#define AES_MAXNR 14

struct aes_key_st {
  uint32_t rd_key[4 * (AES_MAXNR + 1)];
  unsigned rounds;
};
typedef struct aes_key_st AES_KEY;

#endif  
