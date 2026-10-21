#[cfg(target_arch = "x86")]
pub fn shift_full_blocks<const BLOCK_LEN: usize>(
    in_out: super::overlapping::Overlapping<'_, u8>,
    mut transform: impl FnMut(&[u8; BLOCK_LEN]) -> [u8; BLOCK_LEN],
) {
    let (in_out, src) = in_out.into_slice_src_mut();
    let in_out_len = in_out[src.clone()].len();

    for i in (0..in_out_len).step_by(BLOCK_LEN) {
        let block = {
            let input =
                <&[u8; BLOCK_LEN]>::try_from(&in_out[(src.start + i)..][..BLOCK_LEN]).unwrap();
            transform(input)
        };
        let output = <&mut [u8; BLOCK_LEN]>::try_from(&mut in_out[i..][..BLOCK_LEN]).unwrap();
        *output = block;
    }
}
