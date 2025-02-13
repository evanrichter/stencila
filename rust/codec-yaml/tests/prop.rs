use codec::CodecTrait;
use codec_yaml::YamlCodec;
use test_props::{node, proptest::prelude::*, Freedom};
use test_utils::assert_json_eq;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn test(input in node(Freedom::Max, vec![])) {
        let string = YamlCodec::to_string(&input, None).unwrap();
        let output = YamlCodec::from_str(&string, None).unwrap();
        assert_json_eq!(input, output)
    }
}
