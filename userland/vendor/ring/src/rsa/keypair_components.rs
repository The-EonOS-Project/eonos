use super::PublicKeyComponents;


#[derive(Clone, Copy)]
pub struct KeyPairComponents<Public, Private = Public> {
    
    pub public_key: PublicKeyComponents<Public>,

    
    pub d: Private,

    
    pub p: Private,

    
    pub q: Private,

    
    pub dP: Private,

    
    pub dQ: Private,

    
    pub qInv: Private,
}

impl<Public, Private> core::fmt::Debug for KeyPairComponents<Public, Private>
where
    PublicKeyComponents<Public>: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        
        f.debug_struct("KeyPairComponents")
            .field("public_key", &self.public_key)
            .finish()
    }
}
