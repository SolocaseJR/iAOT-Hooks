use crate::errors::IaotError;
fn reg(r: u8) -> Result<u32, IaotError> {
    if r < 32 {
        Ok(r as u32)
    } else {
        Err(IaotError::Unsupported(
            "ARM64 register must be 0..31".into(),
        ))
    }
}
pub fn b(pc: u64, target: u64) -> Result<u32, IaotError> {
    if !pc.is_multiple_of(4) || !target.is_multiple_of(4) {
        return Err(IaotError::Unsupported(
            "branch addresses must be instruction aligned".into(),
        ));
    }
    let delta = target as i128 - pc as i128;
    if !(-(1i128 << 27)..(1i128 << 27)).contains(&delta) {
        return Err(IaotError::Unsupported(
            "B target is outside +/-128 MiB".into(),
        ));
    }
    Ok(0x1400_0000 | (((delta >> 2) as i64 as u32) & 0x03ff_ffff))
}
pub fn blr(r: u8) -> Result<u32, IaotError> {
    Ok(0xd63f_0000 | (reg(r)? << 5))
}
pub fn br(r: u8) -> Result<u32, IaotError> {
    Ok(0xd61f_0000 | (reg(r)? << 5))
}
pub fn add_imm(rd: u8, rn: u8, imm: u16, is64: bool) -> Result<u32, IaotError> {
    if imm > 4095 {
        return Err(IaotError::Unsupported("ADD immediate is too large".into()));
    }
    Ok((if is64 { 0x9100_0000 } else { 0x1100_0000 })
        | ((imm as u32) << 10)
        | (reg(rn)? << 5)
        | reg(rd)?)
}
pub fn sub_imm(rd: u8, rn: u8, imm: u16, is64: bool) -> Result<u32, IaotError> {
    if imm > 4095 {
        return Err(IaotError::Unsupported("SUB immediate is too large".into()));
    }
    Ok((if is64 { 0xd100_0000 } else { 0x5100_0000 })
        | ((imm as u32) << 10)
        | (reg(rn)? << 5)
        | reg(rd)?)
}
pub fn ret() -> u32 {
    0xd65f_03c0
}
pub fn mov(dst: u8, src: u8, is64: bool) -> Result<u32, IaotError> {
    Ok((if is64 { 0xaa00_03e0 } else { 0x2a00_03e0 }) | (reg(src)? << 16) | reg(dst)?)
}
pub fn movz(dst: u8, imm: u16, shift: u8, is64: bool) -> Result<u32, IaotError> {
    if !shift.is_multiple_of(16) || shift > (if is64 { 48 } else { 16 }) {
        return Err(IaotError::Unsupported("invalid MOVZ shift".into()));
    }
    Ok((if is64 { 0xd280_0000 } else { 0x5280_0000 })
        | (((shift / 16) as u32) << 21)
        | ((imm as u32) << 5)
        | reg(dst)?)
}
pub fn cbz(regno: u8, pc: u64, target: u64, is64: bool) -> Result<u32, IaotError> {
    if !pc.is_multiple_of(4) || !target.is_multiple_of(4) {
        return Err(IaotError::Unsupported(
            "CBZ addresses must be aligned".into(),
        ));
    }
    let d = target as i128 - pc as i128;
    if !(-(1i128 << 20)..(1i128 << 20)).contains(&d) {
        return Err(IaotError::Unsupported("CBZ target out of range".into()));
    }
    Ok((if is64 { 0xb400_0000 } else { 0x3400_0000 })
        | ((((d >> 2) as i64 as u32) & 0x7ffff) << 5)
        | reg(regno)?)
}
pub fn ldr_str_imm(rt: u8, rn: u8, offset: u16, is64: bool, load: bool) -> Result<u32, IaotError> {
    let scale = if is64 { 8 } else { 4 };
    if !offset.is_multiple_of(scale) || offset / scale > 4095 {
        return Err(IaotError::Unsupported(
            "LDR/STR offset is not encodable".into(),
        ));
    }
    let base = match (is64, load) {
        (true, true) => 0xf940_0000,
        (true, false) => 0xf900_0000,
        (false, true) => 0xb940_0000,
        (false, false) => 0xb900_0000,
    };
    Ok(base | (((offset / scale) as u32) << 10) | (reg(rn)? << 5) | reg(rt)?)
}
pub fn adrp(rd: u8, pc: u64, target: u64) -> Result<u32, IaotError> {
    let d = ((target & !0xfff) as i128 - (pc & !0xfff) as i128) >> 12;
    if !(-(1i128 << 20)..(1i128 << 20)).contains(&d) {
        return Err(IaotError::Unsupported("ADRP target out of range".into()));
    }
    let v = d as i64 as u64;
    Ok(0x9000_0000 | (((v as u32) & 3) << 29) | ((((v >> 2) as u32) & 0x7ffff) << 5) | reg(rd)?)
}
pub fn stp_ldp(rt: u8, rt2: u8, rn: u8, offset: i16, load: bool) -> Result<u32, IaotError> {
    if offset % 8 != 0 || !(-512..=504).contains(&offset) {
        return Err(IaotError::Unsupported("STP/LDP offset invalid".into()));
    }
    let imm7 = ((offset / 8) as i32 as u32) & 0x7f;
    Ok((if load { 0xa940_0000 } else { 0xa900_0000 })
        | (imm7 << 15)
        | (reg(rt2)? << 10)
        | (reg(rn)? << 5)
        | reg(rt)?)
}
pub fn stp_pre(rt: u8, rt2: u8, rn: u8, offset: i16) -> Result<u32, IaotError> {
    if offset % 8 != 0 || !(-512..=504).contains(&offset) {
        return Err(IaotError::Unsupported(
            "STP pre-index offset invalid".into(),
        ));
    }
    let imm7 = ((offset / 8) as i32 as u32) & 0x7f;
    Ok(0xa980_0000 | (imm7 << 15) | (reg(rt2)? << 10) | (reg(rn)? << 5) | reg(rt)?)
}
pub fn ldp_post(rt: u8, rt2: u8, rn: u8, offset: i16) -> Result<u32, IaotError> {
    if offset % 8 != 0 || !(-512..=504).contains(&offset) {
        return Err(IaotError::Unsupported(
            "LDP post-index offset invalid".into(),
        ));
    }
    let imm7 = ((offset / 8) as i32 as u32) & 0x7f;
    Ok(0xa8c0_0000 | (imm7 << 15) | (reg(rt2)? << 10) | (reg(rn)? << 5) | reg(rt)?)
}
pub fn q_stp_ldp(rt: u8, rt2: u8, rn: u8, offset: u16, load: bool) -> Result<u32, IaotError> {
    if rt > 31 || rt2 > 31 || !offset.is_multiple_of(16) || offset / 16 > 127 {
        return Err(IaotError::Unsupported(
            "Q STP/LDP operand is not encodable".into(),
        ));
    }
    Ok((if load { 0xad40_0000 } else { 0xad00_0000 })
        | (((offset / 16) as u32) << 15)
        | ((rt2 as u32) << 10)
        | (reg(rn)? << 5)
        | rt as u32)
}
pub fn mrs_nzcv(rt: u8) -> Result<u32, IaotError> {
    Ok(0xd53b_4200 | reg(rt)?)
}
pub fn msr_nzcv(rt: u8) -> Result<u32, IaotError> {
    Ok(0xd51b_4200 | reg(rt)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branches() {
        assert_eq!(b(0x1000, 0x1004).unwrap(), 0x14000001);
        assert_eq!(b(0x1004, 0x1000).unwrap(), 0x17ffffff);
        assert!(b(1, 4).is_err());
        assert!(b(0, 0x8000000).is_err());
    }
}
