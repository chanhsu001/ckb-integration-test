mod util;
pub(super) mod v2019;
pub(super) mod v2021;

const ERROR_EMPTY_EXT: &str = "Invalid: Block(EmptyBlockExtension(";
const ERROR_MAX_LIMIT: &str = "Invalid: Block(ExceededMaximumBlockExtensionBytes(";
const ERROR_UNKNOWN_FIELDS: &str = "Invalid: Block(UnknownFields(";
