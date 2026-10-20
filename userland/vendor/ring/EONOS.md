EonOS vendor note
=================

This is upstream ring 0.17.14. The only source change is in `build.rs`: EonOS's
`x86_64-unknown-none-elf` userspace uses the x86-64 System V ELF ABI, so `none`
is mapped to ring's existing ELF/perlasm assembly backend. No cryptographic
code or algorithm selection was changed.

EonOS sets `CFLAGS_x86_64_eonos_user=-fno-stack-protector` when building
userland. The host C compiler enables stack protector instrumentation by
default, but EonOS has no C TLS canary runtime (`%fs:0x28`), so that host default
would introduce an unresolved symbol and an invalid guard access. Rustls uses
ring's normal constant-time implementations and CPU feature checks.

Upstream license notices are retained in `LICENSE`, `LICENSE-BoringSSL`, and
`LICENSE-other-bits`.
