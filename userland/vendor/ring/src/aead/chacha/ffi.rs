













use super::{super::overlapping::Overlapping, Counter, Key};





macro_rules! chacha20_ctr32_ffi {
    ( unsafe { ($MIN_LEN:expr, $Cpu:ty, $InOut:ty) => $f:ident },
      $key:expr, $counter:expr, $in_out:expr, $cpu:expr ) => {{
        prefixed_extern! {
            fn $f(
                out: *mut u8,
                in_: *const u8,
                in_len: crate::c::size_t,
                key: &[u32; 8],
                counter: &crate::aead::chacha::Counter,
            );
        }
        
        
        
        unsafe {
            crate::aead::chacha::ffi::chacha20_ctr32_ffi::<$InOut, $Cpu, $MIN_LEN>(
                $key, $counter, $in_out, $cpu, $f,
            )
        }
    }};
}



pub(super) unsafe fn chacha20_ctr32_ffi<
    'o,
    InOut: 'o + Into<Overlapping<'o, u8>>,
    Cpu,
    const MIN_LEN: usize,
>(
    key: &Key,
    counter: Counter,
    in_out: InOut,
    cpu: Cpu,
    f: unsafe extern "C" fn(*mut u8, *const u8, crate::c::size_t, &[u32; 8], &Counter),
) {
    assert!(MIN_LEN > 0);
    let in_out: Overlapping<'_, u8> = in_out.into();
    in_out.with_input_output_len(|input, output, len| {
        assert!(len >= MIN_LEN);
        let key = key.words_less_safe();
        let _: Cpu = cpu;
        unsafe { f(output, input, len, key, &counter) }
    });
}
