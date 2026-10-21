













use super::Word;
use core::ops;


#[repr(transparent)]
pub struct BoolMask(Word);

impl BoolMask {
    #[cfg(test)]
    pub(super) const TRUE: Self = Self(Word::MAX);
    #[cfg(test)]
    pub(super) const FALSE: Self = Self(0);

    
    
    pub(crate) fn leak(self) -> bool {
        self.0 != 0
    }
}

impl ops::BitAnd for BoolMask {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
