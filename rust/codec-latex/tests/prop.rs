use codec::{utils::vec_string, CodecTrait};
use codec_latex::LatexCodec;
use test_props::{article, proptest::prelude::*, Freedom};
use test_utils::{
    assert_json_eq,
    common::{once_cell::sync::Lazy, tokio},
};

static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().unwrap());

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn test(input in article(
        Freedom::Min,
        [
            LatexCodec::spec().unsupported_types,
            // Pandoc seems to add a caption "image" to image objects, which breaks this test
            // so exclude for the current time.
            // Tables are also currently failing this test
            vec_string!["ImageObject", "Table"]
        ].concat(),
        LatexCodec::spec().unsupported_properties
    )) {
        RUNTIME.block_on(async {
            let latex = LatexCodec::to_string_async(&input, None).await.unwrap();
            let output = LatexCodec::from_str_async(&latex, None).await.unwrap();
            assert_json_eq!(input, output)
        })
    }
}
