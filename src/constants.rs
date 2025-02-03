pub const DEFAULT_CLIENT_SIZE: usize = 1000;
pub const ASCII_LINE_FEED: u8 = 10;
pub const ASCII_CARRIAGE_RETURN: u8 = 13;
pub const ASCII_ASTERISK: u8 = 42;
pub const ASCII_BULK_STRING: u8 = 36;
pub const RESP_BUFFER_SIZE: usize = 4096;
pub const RESP_COMMAND_ARG_SIZE: usize = 100;

pub const OKAY_RESPONSE: &str = "%7\r\n\
+server\r\n\
+infinity_q\r\n\
+version\r\n\
:1\r\n\
+proto\r\n\
:3\r\n\
+id\r\n\
$1\r\n\
a\r\n\
+mode\r\n\
$10\r\n\
standalone\r\n\
+role\r\n\
$6\r\n\
master\r\n\
+modules\r\n\
*-1\r\n";
