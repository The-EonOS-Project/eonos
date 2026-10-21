






























use super::{
    chacha::{self, *},
    chacha20_poly1305, cpu, poly1305, Aad, Nonce, Tag,
};
use crate::{
    bb,
    error::{self, InputTooLongError},
    polyfill::slice,
};


pub struct SealingKey {
    key: Key,
}

impl SealingKey {
    
    pub fn new(key_material: &[u8; KEY_LEN]) -> Self {
        Self {
            key: Key::new(key_material),
        }
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    pub fn seal_in_place(
        &self,
        sequence_number: u32,
        plaintext_in_ciphertext_out: &mut [u8],
        tag_out: &mut [u8; TAG_LEN],
    ) {
        
        let (len_in_out, data_and_padding_in_out): (&mut [u8; PACKET_LENGTH_LEN], _) =
            slice::split_first_chunk_mut(plaintext_in_ciphertext_out).unwrap();

        let cpu = cpu::features();
        
        let (counter, poly_key) = chacha20_poly1305::begin(
            &self.key.k_2,
            make_nonce(sequence_number),
            Aad::from(len_in_out),
            data_and_padding_in_out,
            cpu,
        )
        .map_err(error::erase::<InputTooLongError>)
        .unwrap();

        let _: Counter = self.key.k_1.encrypt_single_block_with_ctr_0(
            make_nonce(sequence_number),
            len_in_out,
            cpu,
        );
        self.key
            .k_2
            .encrypt(counter, data_and_padding_in_out.into(), cpu);

        let Tag(tag) = poly1305::sign(poly_key, plaintext_in_ciphertext_out, cpu);
        *tag_out = tag;
    }
}


pub struct OpeningKey {
    key: Key,
}

impl OpeningKey {
    
    pub fn new(key_material: &[u8; KEY_LEN]) -> Self {
        Self {
            key: Key::new(key_material),
        }
    }

    
    
    
    
    pub fn decrypt_packet_length(
        &self,
        sequence_number: u32,
        encrypted_packet_length: [u8; PACKET_LENGTH_LEN],
    ) -> [u8; PACKET_LENGTH_LEN] {
        let cpu = cpu::features();
        let mut packet_length = encrypted_packet_length;
        let _: Counter = self.key.k_1.encrypt_single_block_with_ctr_0(
            make_nonce(sequence_number),
            &mut packet_length,
            cpu,
        );
        packet_length
    }

    
    
    
    
    
    
    
    
    
    pub fn open_in_place<'a>(
        &self,
        sequence_number: u32,
        ciphertext_in_plaintext_out: &'a mut [u8],
        tag: &[u8; TAG_LEN],
    ) -> Result<&'a [u8], error::Unspecified> {
        let (packet_length, after_packet_length): (&mut [u8; PACKET_LENGTH_LEN], _) =
            slice::split_first_chunk_mut(ciphertext_in_plaintext_out).ok_or(error::Unspecified)?;

        let cpu = cpu::features();
        let (counter, poly_key) = chacha20_poly1305::begin(
            &self.key.k_2,
            make_nonce(sequence_number),
            Aad::from(packet_length),
            after_packet_length,
            cpu,
        )
        .map_err(error::erase::<InputTooLongError>)?;

        
        
        
        let calculated_tag = poly1305::sign(poly_key, ciphertext_in_plaintext_out, cpu);
        bb::verify_slices_are_equal(calculated_tag.as_ref(), tag)?;

        
        let after_packet_length = &mut ciphertext_in_plaintext_out[PACKET_LENGTH_LEN..];

        self.key
            .k_2
            .encrypt(counter, after_packet_length.into(), cpu);

        Ok(after_packet_length)
    }
}

struct Key {
    k_1: chacha::Key,
    k_2: chacha::Key,
}

impl Key {
    fn new(key_material: &[u8; KEY_LEN]) -> Self {
        
        let (k_2, k_1) = key_material.split_at(chacha::KEY_LEN);
        Self {
            k_1: chacha::Key::new(k_1.try_into().unwrap()),
            k_2: chacha::Key::new(k_2.try_into().unwrap()),
        }
    }
}

fn make_nonce(sequence_number: u32) -> Nonce {
    let [s0, s1, s2, s3] = sequence_number.to_be_bytes();
    let nonce = [0, 0, 0, 0, 0, 0, 0, 0, s0, s1, s2, s3];
    Nonce::assume_unique_for_key(nonce)
}


pub const KEY_LEN: usize = chacha::KEY_LEN * 2;


pub const PACKET_LENGTH_LEN: usize = 4; 


pub const TAG_LEN: usize = super::TAG_LEN;
