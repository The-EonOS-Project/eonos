






















































































































extern crate alloc;

use alloc::{format, string::String, vec::Vec};

use crate::{bits, digest, error};

#[cfg(any(feature = "std", feature = "test_logging"))]
extern crate std;



pub const fn compile_time_assert_clone<T: Clone>() {}



pub const fn compile_time_assert_copy<T: Copy>() {}



pub const fn compile_time_assert_eq<T: Eq>() {}



pub const fn compile_time_assert_send<T: Send>() {}



pub const fn compile_time_assert_sync<T: Sync>() {}



#[cfg(feature = "std")]
pub const fn compile_time_assert_std_error_error<T: std::error::Error>() {}






#[derive(Debug)]
pub struct TestCase {
    attributes: Vec<(String, String, bool)>,
}

impl TestCase {
    
    pub fn consume_bool(&mut self, key: &str) -> bool {
        match self.consume_string(key).as_ref() {
            "true" => true,
            "false" => false,
            s => panic!("Invalid bool value: {}", s),
        }
    }

    
    
    
    
    
    pub fn consume_digest_alg(&mut self, key: &str) -> Option<&'static digest::Algorithm> {
        let name = self.consume_string(key);
        match name.as_ref() {
            "SHA1" => Some(&digest::SHA1_FOR_LEGACY_USE_ONLY),
            "SHA224" => None, 
            "SHA256" => Some(&digest::SHA256),
            "SHA384" => Some(&digest::SHA384),
            "SHA512" => Some(&digest::SHA512),
            "SHA512_256" => Some(&digest::SHA512_256),
            _ => panic!("Unsupported digest algorithm: {}", name),
        }
    }

    
    
    
    pub fn consume_bytes(&mut self, key: &str) -> Vec<u8> {
        self.consume_optional_bytes(key)
            .unwrap_or_else(|| panic!("No attribute named \"{}\"", key))
    }

    
    
    pub fn consume_optional_bytes(&mut self, key: &str) -> Option<Vec<u8>> {
        let s = self.consume_optional_string(key)?;
        let result = if let [b'\"', s @ ..] = s.as_bytes() {
            
            let mut s = s.iter();
            let mut bytes = Vec::with_capacity(s.len() - 1);
            loop {
                let b = match s.next() {
                    Some(b'\\') => {
                        match s.next() {
                            
                            Some(b'0') => 0u8,
                            Some(b't') => b'\t',
                            Some(b'n') => b'\n',
                            
                            Some(b'x') => {
                                let hi = s.next().expect("Invalid hex escape sequence in string.");
                                let lo = s.next().expect("Invalid hex escape sequence in string.");
                                if let (Ok(hi), Ok(lo)) = (from_hex_digit(*hi), from_hex_digit(*lo))
                                {
                                    (hi << 4) | lo
                                } else {
                                    panic!("Invalid hex escape sequence in string.");
                                }
                            }
                            _ => {
                                panic!("Invalid hex escape sequence in string.");
                            }
                        }
                    }
                    Some(b'"') => {
                        if s.next().is_some() {
                            panic!("characters after the closing quote of a quoted string.");
                        }
                        break;
                    }
                    Some(b) => *b,
                    None => panic!("Missing terminating '\"' in string literal."),
                };
                bytes.push(b);
            }
            bytes
        } else {
            
            match from_hex(&s) {
                Ok(s) => s,
                Err(err_str) => {
                    panic!("{} in {}", err_str, s);
                }
            }
        };
        Some(result)
    }

    
    
    pub fn consume_usize(&mut self, key: &str) -> usize {
        let s = self.consume_string(key);
        s.parse::<usize>().unwrap()
    }

    
    
    pub fn consume_usize_bits(&mut self, key: &str) -> bits::BitLength {
        let s = self.consume_string(key);
        let bits = s.parse::<usize>().unwrap();
        bits::BitLength::from_bits(bits)
    }

    
    
    pub fn consume_string(&mut self, key: &str) -> String {
        self.consume_optional_string(key)
            .unwrap_or_else(|| panic!("No attribute named \"{}\"", key))
    }

    
    
    pub fn consume_optional_string(&mut self, key: &str) -> Option<String> {
        for (name, value, consumed) in &mut self.attributes {
            if key == name {
                if *consumed {
                    panic!("Attribute {} was already consumed", key);
                }
                *consumed = true;
                return Some(value.clone());
            }
        }
        None
    }
}


#[cfg(test)]
macro_rules! test_vector_file {
    ($file_name:expr) => {
        $crate::testutil::File {
            file_name: $file_name,
            contents: include_str!($file_name),
        }
    };
}


pub struct File<'a> {
    
    pub file_name: &'a str,

    
    pub contents: &'a str,
}




pub fn run<F>(test_file: File, mut f: F)
where
    F: FnMut(&str, &mut TestCase) -> Result<(), error::Unspecified>,
{
    let lines = &mut test_file.contents.lines();

    let mut current_section = String::from("");
    let mut failed = false;

    while let Some(mut test_case) = parse_test_case(&mut current_section, lines) {
        let result = match f(&current_section, &mut test_case) {
            Ok(()) => {
                if !test_case
                    .attributes
                    .iter()
                    .any(|&(_, _, consumed)| !consumed)
                {
                    Ok(())
                } else {
                    failed = true;
                    Err("Test didn't consume all attributes.")
                }
            }
            Err(error::Unspecified) => Err("Test returned Err(error::Unspecified)."),
        };

        if result.is_err() {
            failed = true;
        }

        #[cfg(feature = "test_logging")]
        if let Err(msg) = result {
            std::println!("{}: {}", test_file.file_name, msg);

            for (name, value, consumed) in test_case.attributes {
                let consumed_str = if consumed { "" } else { " (unconsumed)" };
                std::println!("{}{} = {}", name, consumed_str, value);
            }
        };
    }

    if failed {
        panic!("Test failed.")
    }
}



pub fn from_hex(hex_str: &str) -> Result<Vec<u8>, String> {
    if hex_str.len() % 2 != 0 {
        return Err(String::from(
            "Hex string does not have an even number of digits",
        ));
    }

    let mut result = Vec::with_capacity(hex_str.len() / 2);
    for digits in hex_str.as_bytes().chunks(2) {
        let hi = from_hex_digit(digits[0])?;
        let lo = from_hex_digit(digits[1])?;
        result.push((hi * 0x10) | lo);
    }
    Ok(result)
}

fn from_hex_digit(d: u8) -> Result<u8, String> {
    use core::ops::RangeInclusive;
    const DECIMAL: (u8, RangeInclusive<u8>) = (0, b'0'..=b'9');
    const HEX_LOWER: (u8, RangeInclusive<u8>) = (10, b'a'..=b'f');
    const HEX_UPPER: (u8, RangeInclusive<u8>) = (10, b'A'..=b'F');
    for (offset, range) in &[DECIMAL, HEX_LOWER, HEX_UPPER] {
        if range.contains(&d) {
            return Ok(d - range.start() + offset);
        }
    }
    Err(format!("Invalid hex digit '{}'", d as char))
}

fn parse_test_case(
    current_section: &mut String,
    lines: &mut dyn Iterator<Item = &str>,
) -> Option<TestCase> {
    let mut attributes = Vec::new();

    let mut is_first_line = true;
    loop {
        let line = lines.next();

        #[cfg(feature = "test_logging")]
        if let Some(text) = &line {
            std::println!("Line: {}", text);
        }

        match line {
            
            
            None if is_first_line => {
                return None;
            }

            
            None => {
                return Some(TestCase { attributes });
            }

            
            Some("") => {
                if !is_first_line {
                    return Some(TestCase { attributes });
                }
                
            }

            
            Some(line) if line.starts_with('#') => (),

            Some(line) if line.starts_with('[') => {
                assert!(is_first_line);
                assert!(line.ends_with(']'));
                current_section.truncate(0);
                current_section.push_str(line);
                let _ = current_section.pop();
                let _ = current_section.remove(0);
            }

            Some(line) => {
                is_first_line = false;

                let parts: Vec<&str> = line.splitn(2, " = ").collect();
                if parts.len() != 2 {
                    panic!("Syntax error: Expected Key = Value.");
                };

                let key = parts[0].trim();
                let value = parts[1].trim();

                
                
                assert_ne!(value.len(), 0);

                
                attributes.push((String::from(key), String::from(value), false));
            }
        }
    }
}







#[doc(hidden)]
pub mod rand {
    use crate::{error, rand};

    
    
    #[derive(Debug)]
    pub struct FixedByteRandom {
        pub byte: u8,
    }

    impl rand::sealed::SecureRandom for FixedByteRandom {
        fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
            dest.fill(self.byte);
            Ok(())
        }
    }

    
    
    
    #[derive(Debug)]
    pub struct FixedSliceRandom<'a> {
        pub bytes: &'a [u8],
    }

    impl rand::sealed::SecureRandom for FixedSliceRandom<'_> {
        fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
            dest.copy_from_slice(self.bytes);
            Ok(())
        }
    }

    
    
    
    
    
    
    
    
    
    #[derive(Debug)]
    pub struct FixedSliceSequenceRandom<'a> {
        
        pub bytes: &'a [&'a [u8]],
        pub current: core::cell::UnsafeCell<usize>,
    }

    impl rand::sealed::SecureRandom for FixedSliceSequenceRandom<'_> {
        fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
            let current = unsafe { *self.current.get() };
            let bytes = self.bytes[current];
            dest.copy_from_slice(bytes);
            
            
            unsafe { *self.current.get() += 1 };
            Ok(())
        }
    }

    impl Drop for FixedSliceSequenceRandom<'_> {
        fn drop(&mut self) {
            
            
            assert_eq!(unsafe { *self.current.get() }, self.bytes.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error;
    use crate::testutil as test;

    #[test]
    fn one_ok() {
        test::run(test_vector_file!("test_1_tests.txt"), |_, test_case| {
            let _ = test_case.consume_string("Key");
            Ok(())
        });
    }

    #[test]
    #[should_panic(expected = "Test failed.")]
    fn one_err() {
        test::run(test_vector_file!("test_1_tests.txt"), |_, test_case| {
            let _ = test_case.consume_string("Key");
            Err(error::Unspecified)
        });
    }

    #[test]
    #[should_panic(expected = "Oh noes!")]
    fn one_panics() {
        test::run(test_vector_file!("test_1_tests.txt"), |_, test_case| {
            let _ = test_case.consume_string("Key");
            panic!("Oh noes!");
        });
    }

    #[test]
    #[should_panic(expected = "Test failed.")]
    fn first_err() {
        err_one(0)
    }

    #[test]
    #[should_panic(expected = "Test failed.")]
    fn middle_err() {
        err_one(1)
    }

    #[test]
    #[should_panic(expected = "Test failed.")]
    fn last_err() {
        err_one(2)
    }

    fn err_one(test_to_fail: usize) {
        let mut n = 0;
        test::run(test_vector_file!("test_3_tests.txt"), |_, test_case| {
            let _ = test_case.consume_string("Key");
            let result = if n != test_to_fail {
                Ok(())
            } else {
                Err(error::Unspecified)
            };
            n += 1;
            result
        });
    }

    #[test]
    #[should_panic(expected = "Oh Noes!")]
    fn first_panic() {
        panic_one(0)
    }

    #[test]
    #[should_panic(expected = "Oh Noes!")]
    fn middle_panic() {
        panic_one(1)
    }

    #[test]
    #[should_panic(expected = "Oh Noes!")]
    fn last_panic() {
        panic_one(2)
    }

    fn panic_one(test_to_fail: usize) {
        let mut n = 0;
        test::run(test_vector_file!("test_3_tests.txt"), |_, test_case| {
            let _ = test_case.consume_string("Key");
            if n == test_to_fail {
                panic!("Oh Noes!");
            };
            n += 1;
            Ok(())
        });
    }

    #[test]
    #[should_panic(expected = "Syntax error: Expected Key = Value.")]
    fn syntax_error() {
        test::run(
            test_vector_file!("test_1_syntax_error_tests.txt"),
            |_, _| Ok(()),
        );
    }
}
