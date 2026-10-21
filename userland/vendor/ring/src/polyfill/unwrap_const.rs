pub const fn unwrap_const<T>(x: Option<T>) -> T
where
    T: Copy,
{
    if let Some(x) = x {
        x
    } else {
        panic!("unwrap_const on `None`");
    }
}
