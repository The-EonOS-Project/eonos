use crate::error;
use crate::polyfill::{unwrap_const, ArrayFlatMap, LeadingZerosStripped};
use core::num::NonZeroU64;


#[derive(Clone, Copy)]
pub struct PublicExponent(NonZeroU64);

impl PublicExponent {
    #[cfg(test)]
    const ALL_CONSTANTS: [Self; 3] = [Self::_3, Self::_65537, Self::MAX];

    pub(super) const _3: Self = Self(unwrap_const(NonZeroU64::new(3)));
    pub(super) const _65537: Self = Self(unwrap_const(NonZeroU64::new(65537)));

    
    
    
    
    
    
    
    
    
    
    
    const MAX: Self = Self(unwrap_const(NonZeroU64::new((1u64 << 33) - 1)));

    pub(super) fn from_be_bytes(
        input: untrusted::Input,
        min_value: Self,
    ) -> Result<Self, error::KeyRejected> {
        
        

        if input.len() > 5 {
            return Err(error::KeyRejected::too_large());
        }
        let value = input.read_all(error::KeyRejected::invalid_encoding(), |input| {
            
            
            if input.peek(0) {
                return Err(error::KeyRejected::invalid_encoding());
            }
            let mut value = 0u64;
            loop {
                let byte = input
                    .read_byte()
                    .map_err(|untrusted::EndOfInput| error::KeyRejected::invalid_encoding())?;
                value = (value << 8) | u64::from(byte);
                if input.at_end() {
                    return Ok(value);
                }
            }
        })?;

        
        
        
        
        let value = NonZeroU64::new(value).ok_or_else(error::KeyRejected::too_small)?;
        if value < min_value.0 {
            return Err(error::KeyRejected::too_small());
        }
        if value > Self::MAX.0 {
            return Err(error::KeyRejected::too_large());
        }

        
        if value.get() & 1 != 1 {
            return Err(error::KeyRejected::invalid_component());
        }

        Ok(Self(value))
    }

    
    
    
    pub fn be_bytes(&self) -> impl ExactSizeIterator<Item = u8> + Clone + '_ {
        
        let bytes = ArrayFlatMap::new(core::iter::once(self.0.get()), u64::to_be_bytes).unwrap();
        LeadingZerosStripped::new(bytes)
    }

    pub(super) fn value(self) -> NonZeroU64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_exponent_constants() {
        for value in PublicExponent::ALL_CONSTANTS.iter() {
            let value: u64 = value.0.into();
            assert_eq!(value & 1, 1);
            assert!(value >= PublicExponent::_3.0.into()); 
            assert!(value <= PublicExponent::MAX.0.into());
        }
    }
}
