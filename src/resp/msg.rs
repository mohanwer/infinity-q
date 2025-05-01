use crate::resp::result::RespError::UTFConversionError;
use crate::resp::result::*;

#[derive(Debug, Clone)]
pub struct RespMsg {
    pub cmd_name: String,
    pub args: Vec<Vec<u8>>,
}

impl RespMsg {
    pub fn new(mut cmd_arg_bytes: Vec<Vec<u8>>) -> Result<Self> {
        let cmd_name =
            String::from_utf8(cmd_arg_bytes[0].clone()).map_err(|_| UTFConversionError)?;
        let mut args = Vec::with_capacity(cmd_arg_bytes.len() - 2);
        for i in 1..cmd_arg_bytes.len() {
            let arg = std::mem::take(&mut cmd_arg_bytes[i]);
            args.push(arg);
        }
        Ok(RespMsg { cmd_name, args })
    }

    pub fn arg_to_str(&self, index: usize) -> Result<String> {
        let arg = String::from_utf8(self.args[index].clone()).map_err(|_| UTFConversionError)?;
        Ok(arg)
    }

    pub fn arg_to_u32(&self, index: usize) -> Result<u32> {
        let arg_str = self.arg_to_str(index)?;
        let arg = u32::from_str_radix(&arg_str, 10).map_err(|_| UTFConversionError)?;
        Ok(arg)
    }

    pub fn args_to_str_vec(&self) -> Result<Vec<String>> {
        let mut args: Vec<String> = Vec::with_capacity(self.args.len());
        for i in 0..self.args.len() {
            let arg = self.arg_to_str(i)?;
            args.push(arg);
        }
        Ok(args)
    }
}
