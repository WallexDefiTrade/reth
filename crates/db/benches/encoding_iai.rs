use iai::{black_box, main};
use reth_interfaces::db;

/// Benchmarks the encoding and decoding of `Header` using iai.
macro_rules! impl_iai_encoding_benchmark {
    ($name:tt) => {
        fn $name() {
            db::codecs::fuzz::Header::encode_and_decode(black_box(
                reth_primitives::Header::default(),
            ));
        }

        main!($name);
    };
}

#[cfg(not(feature = "bench-postcard"))]
impl_iai_encoding_benchmark!(scale);

#[cfg(feature = "bench-postcard")]
impl_iai_encoding_benchmark!(postcard);
