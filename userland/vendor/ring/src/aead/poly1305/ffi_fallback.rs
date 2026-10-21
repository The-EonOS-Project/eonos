














use super::{Key, Tag, KEY_LEN, TAG_LEN};
use crate::c;
use core::num::NonZeroUsize;


pub(in super::super) struct State {
    state: poly1305_state_st,
}


#[repr(C, align(64))]
struct poly1305_state_st {
    r0: u32,
    r1: u32,
    r2: u32,
    r3: u32,
    r4: u32,
    s1: u32,
    s2: u32,
    s3: u32,
    s4: u32,
    h0: u32,
    h1: u32,
    h2: u32,
    h3: u32,
    h4: u32,
    key: [u8; 16],
}

impl State {
    pub(super) fn new_context(Key { key_and_nonce }: Key) -> super::Context {
        prefixed_extern! {
            fn CRYPTO_poly1305_init(state: &mut poly1305_state_st, key: &[u8; KEY_LEN]);
        }
        let mut r = Self {
            state: poly1305_state_st {
                r0: 0,
                r1: 0,
                r2: 0,
                r3: 0,
                r4: 0,
                s1: 0,
                s2: 0,
                s3: 0,
                s4: 0,
                h0: 0,
                h1: 0,
                h2: 0,
                h3: 0,
                h4: 0,
                key: [0u8; 16],
            },
        };
        unsafe { CRYPTO_poly1305_init(&mut r.state, &key_and_nonce) }
        super::Context::Fallback(r)
    }

    
    
    pub(super) fn update_internal(&mut self, input: &[u8]) {
        prefixed_extern! {
            fn CRYPTO_poly1305_update(
                state: &mut poly1305_state_st,
                input: *const u8,
                in_len: c::NonZero_size_t);
        }
        if let Some(len) = NonZeroUsize::new(input.len()) {
            let input = input.as_ptr();
            unsafe { CRYPTO_poly1305_update(&mut self.state, input, len) }
        }
    }

    pub(super) fn finish(mut self) -> Tag {
        prefixed_extern! {
            fn CRYPTO_poly1305_finish(statep: &mut poly1305_state_st, mac: &mut [u8; TAG_LEN]);
        }
        let mut tag = Tag([0u8; TAG_LEN]);
        unsafe { CRYPTO_poly1305_finish(&mut self.state, &mut tag.0) }
        tag
    }
}
