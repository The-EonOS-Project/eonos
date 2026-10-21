use crate::{
    arithmetic::{bigint, montgomery::RR},
    bits::{self, FromByteLen as _},
    cpu,
    error::{self, InputTooLongError},
    rsa::N,
};
use core::ops::RangeInclusive;


pub struct PublicModulus {
    value: bigint::OwnedModulus<N>,
    oneRR: bigint::One<N, RR>,
}

impl Clone for PublicModulus {
    fn clone(&self) -> Self {
        let PublicModulus { value, oneRR } = self;
        let value = value.clone();

        
        
        let cpu = cpu::features();
        let n = value.modulus(cpu);
        let oneRR = oneRR.clone_into(n.alloc_zero());

        Self { value, oneRR }
    }
}

 




 

impl PublicModulus {
    pub(super) fn from_be_bytes(
        n: untrusted::Input,
        allowed_bit_lengths: RangeInclusive<bits::BitLength>,
        cpu_features: cpu::Features,
    ) -> Result<Self, error::KeyRejected> {
        
        

        let min_bits = *allowed_bit_lengths.start();
        let max_bits = *allowed_bit_lengths.end();

        
        
        
        const MIN_BITS: bits::BitLength = bits::BitLength::from_bits(1024);

        
        let value = bigint::OwnedModulusValue::from_be_bytes(n)?;
        let bits = value.len_bits();

        
        
        
        assert!(min_bits >= MIN_BITS);
        let bits_rounded_up = bits::BitLength::from_byte_len(bits.as_usize_bytes_rounded_up())
            .map_err(error::erase::<InputTooLongError>)
            .unwrap(); 
        if bits_rounded_up < min_bits {
            return Err(error::KeyRejected::too_small());
        }
        if bits > max_bits {
            return Err(error::KeyRejected::too_large());
        }
        let value = bigint::OwnedModulus::from(value);
        let m = value.modulus(cpu_features);
        let oneRR = bigint::One::newRR(m.alloc_zero(), &m);

        Ok(Self { value, oneRR })
    }

    
    
    
    pub fn be_bytes(&self) -> impl ExactSizeIterator<Item = u8> + Clone + '_ {
        self.value.be_bytes()
    }

    
    pub fn len_bits(&self) -> bits::BitLength {
        self.value.len_bits()
    }

    pub(super) fn value(&self, cpu_features: cpu::Features) -> bigint::Modulus<N> {
        self.value.modulus(cpu_features)
    }

    pub(super) fn oneRR(&self) -> &bigint::Elem<N, RR> {
        self.oneRR.as_ref()
    }
}
