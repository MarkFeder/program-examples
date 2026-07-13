use alloc::vec::Vec;

use pinocchio::error::ProgramError;

/// Reads a Borsh `string` (a 4-byte little-endian length prefix followed by that
/// many UTF-8 bytes) starting at `*offset`, advancing `offset` past it.
pub fn read_borsh_string<'a>(data: &'a [u8], offset: &mut usize) -> Result<&'a [u8], ProgramError> {
    let len_bytes: [u8; 4] = data
        .get(*offset..*offset + 4)
        .ok_or(ProgramError::InvalidInstructionData)?
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    *offset += 4;

    let bytes = data
        .get(*offset..*offset + len)
        .ok_or(ProgramError::InvalidInstructionData)?;
    *offset += len;
    Ok(bytes)
}

/// Appends a Borsh `string` (4-byte little-endian length prefix + UTF-8 bytes).
pub fn push_borsh_string(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(&(value.len() as u32).to_le_bytes());
    buffer.extend_from_slice(value);
}
