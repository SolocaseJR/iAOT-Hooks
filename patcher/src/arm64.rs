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
pub fn bl(pc: u64, target: u64) -> Result<u32, IaotError> {
    Ok((b(pc, target)? & 0x03ff_ffff) | 0x9400_0000)
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
pub fn ldrsw_imm(rt: u8, rn: u8, offset: u16) -> Result<u32, IaotError> {
    if !offset.is_multiple_of(4) || offset / 4 > 4095 {
        return Err(IaotError::Unsupported(
            "LDRSW offset is not encodable".into(),
        ));
    }
    Ok(0xb980_0000 | (((offset / 4) as u32) << 10) | (reg(rn)? << 5) | reg(rt)?)
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

fn sign_extend(value: u64, bits: u8) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}

fn branch_target(pc: u64, encoded: u64, bits: u8, scale: u8) -> u64 {
    (pc as i128 + ((sign_extend(encoded, bits) as i128) << scale)) as u64
}

/// Relocate one ARM64 instruction from `original_pc` to `new_pc`.
///
/// The offline bridge currently displaces exactly one instruction. Supported
/// PC-relative families are B/BL, B.cond, CBZ/CBNZ, TBZ/TBNZ, ADR, ADRP and
/// integer literal loads. SIMD literal loads and prefetch literals fail closed.
pub fn relocate(instruction: u32, original_pc: u64, new_pc: u64) -> Result<Vec<u32>, IaotError> {
    if !original_pc.is_multiple_of(4) || !new_pc.is_multiple_of(4) {
        return Err(IaotError::Unsupported(
            "relocation addresses must be aligned".into(),
        ));
    }
    if instruction & 0x7c00_0000 == 0x1400_0000 {
        let target = branch_target(original_pc, (instruction & 0x03ff_ffff) as u64, 26, 2);
        return Ok(vec![if instruction & 0x8000_0000 != 0 {
            bl(new_pc, target)?
        } else {
            b(new_pc, target)?
        }]);
    }
    if instruction & 0xff00_0010 == 0x5400_0000 {
        let target = branch_target(original_pc, ((instruction >> 5) & 0x7ffff) as u64, 19, 2);
        let inverse = (instruction & !0x00ff_ffe0) ^ 1;
        return Ok(vec![inverse | (2 << 5), b(new_pc + 4, target)?]);
    }
    if instruction & 0x7e00_0000 == 0x3400_0000 {
        let target = branch_target(original_pc, ((instruction >> 5) & 0x7ffff) as u64, 19, 2);
        let inverse = (instruction & !0x00ff_ffe0) ^ 0x0100_0000;
        return Ok(vec![inverse | (2 << 5), b(new_pc + 4, target)?]);
    }
    if instruction & 0x7e00_0000 == 0x3600_0000 {
        let target = branch_target(original_pc, ((instruction >> 5) & 0x3fff) as u64, 14, 2);
        let inverse = (instruction & !0x0007_ffe0) ^ 0x0100_0000;
        return Ok(vec![inverse | (2 << 5), b(new_pc + 4, target)?]);
    }
    if instruction & 0x9f00_0000 == 0x1000_0000 {
        let immediate = (((instruction >> 5) & 0x7ffff) << 2) | ((instruction >> 29) & 3);
        let target = (original_pc as i128 + sign_extend(immediate as u64, 21) as i128) as u64;
        let rd = (instruction & 31) as u8;
        return Ok(vec![
            adrp(rd, new_pc, target)?,
            add_imm(rd, rd, (target & 0xfff) as u16, true)?,
        ]);
    }
    if instruction & 0x9f00_0000 == 0x9000_0000 {
        let immediate = (((instruction >> 5) & 0x7ffff) << 2) | ((instruction >> 29) & 3);
        let target = ((original_pc & !0xfff) as i128
            + ((sign_extend(immediate as u64, 21) as i128) << 12)) as u64;
        return Ok(vec![adrp((instruction & 31) as u8, new_pc, target)?]);
    }
    if instruction & 0x3b00_0000 == 0x1800_0000 {
        if instruction & 0x0400_0000 != 0 {
            return Err(IaotError::Unsupported(
                "SIMD/FP literal-load relocation is not implemented".into(),
            ));
        }
        let operation = (instruction >> 30) & 3;
        if operation == 3 {
            return Err(IaotError::Unsupported(
                "PRFM literal relocation is not implemented".into(),
            ));
        }
        let target = branch_target(original_pc, ((instruction >> 5) & 0x7ffff) as u64, 19, 2);
        let rt = (instruction & 31) as u8;
        if rt == 31 {
            return Err(IaotError::Unsupported(
                "literal relocation has no safe destination register".into(),
            ));
        }
        let mut result = vec![
            adrp(rt, new_pc, target)?,
            add_imm(rt, rt, (target & 0xfff) as u16, true)?,
        ];
        result.push(match operation {
            0 => ldr_str_imm(rt, rt, 0, false, true)?,
            1 => ldr_str_imm(rt, rt, 0, true, true)?,
            2 => ldrsw_imm(rt, rt, 0)?,
            _ => unreachable!(),
        });
        return Ok(result);
    }
    Ok(vec![instruction])
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
    #[test]
    fn relocates_pc_relative_families() {
        assert_eq!(
            relocate(0x9400_0002, 0x1000, 0x2000).unwrap(),
            vec![0x97ff_fc02]
        );
        let adr = relocate(0x1000_0080, 0x1000, 0x3000).unwrap();
        assert_eq!(adr.len(), 2);
        let conditional = relocate(0x5400_0040, 0x1000, 0x2000).unwrap();
        assert_eq!(conditional.len(), 2);
        assert!(relocate(0x1c00_0000, 0x1000, 0x2000).is_err());
    }
}
