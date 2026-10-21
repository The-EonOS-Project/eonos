macro_rules! cold_exhaustive_error {

    {
        struct $mod_name:ident::$Error:ident with $vis:vis constructor {
            $field:ident: $ValueType:ty
        }
    } => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*; 

            pub struct $Error { #[allow(dead_code)] $field: $ValueType }

            impl $Error {
                #[cold]
                #[inline(never)]
                $vis fn new($field: $ValueType) -> Self {
                    Self { $field }
                }
            }
        }
    };

    {
        struct $mod_name:ident::$Error:ident {
            $field:ident: $ValueType:ty
        }
    } => {
        cold_exhaustive_error! {
            struct $mod_name::$Error with pub(super) constructor {
                $field: $ValueType
            }
        }
    };

    {
        enum $mod_name:ident::$Error:ident {
            $(
                $constructor:ident => $Variant:ident($ValueType:ty),
            )+
        }
    } => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*; 

            pub enum $Error {
                $(
                    $Variant(#[allow(dead_code)] $ValueType)
                ),+
            }

            impl $Error {
                $(
                    #[cold]
                    #[inline(never)]
                    pub(super) fn $constructor(value: $ValueType) -> Self {
                        Self::$Variant(value)
                    }
                )+
            }
        }
    };
}
