use crate::errors::IaotError;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern(pub Vec<Option<u8>>);
pub fn parse(input: &str) -> Result<Vec<Pattern>, IaotError> {
    input
        .split("||")
        .map(|alt| {
            let bytes = alt
                .split_whitespace()
                .map(|t| {
                    if t == "?" || t == "??" {
                        Ok(None)
                    } else if t.len() == 2 {
                        u8::from_str_radix(t, 16)
                            .map(Some)
                            .map_err(|_| IaotError::Pattern(format!("bad token {t}")))
                    } else {
                        Err(IaotError::Pattern(format!("bad token {t}")))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if bytes.is_empty() {
                Err(IaotError::Pattern("empty alternative".into()))
            } else {
                Ok(Pattern(bytes))
            }
        })
        .collect()
}
#[cfg(test)]
pub fn find(data: &[u8], p: &Pattern) -> Vec<usize> {
    if p.0.len() > data.len() {
        return vec![];
    }
    (0..=data.len() - p.0.len())
        .filter(|&i| {
            p.0.iter()
                .enumerate()
                .all(|(j, b)| b.is_none_or(|v| data[i + j] == v))
        })
        .collect()
}
/// Like `find`, but only tests offsets that are multiples of `align`
/// (instruction starts when `data` itself starts on an instruction).
pub fn find_aligned(data: &[u8], p: &Pattern, align: usize) -> Vec<usize> {
    if p.0.len() > data.len() || align == 0 {
        return vec![];
    }
    (0..=data.len() - p.0.len())
        .step_by(align)
        .filter(|&i| {
            p.0.iter()
                .enumerate()
                .all(|(j, b)| b.is_none_or(|v| data[i + j] == v))
        })
        .collect()
}
/// Target of a B or BL (a call or a tail call).
pub fn resolve_branch_target(instruction: u32, pc: u64) -> Result<u64, IaotError> {
    if instruction & 0x7c00_0000 != 0x1400_0000 {
        return Err(IaotError::Pattern("xref instruction is not B or BL".into()));
    }
    if !pc.is_multiple_of(4) {
        return Err(IaotError::Pattern("xref address is unaligned".into()));
    }
    let imm26 = (instruction & 0x03ff_ffff) as i64;
    let signed = (imm26 << 38) >> 38;
    Ok((pc as i128 + (signed as i128) * 4) as u64)
}
pub fn resolve_bl_target(instruction: u32, pc: u64) -> Result<u64, IaotError> {
    if instruction & 0xfc00_0000 != 0x9400_0000 {
        return Err(IaotError::Pattern("xref instruction is not BL".into()));
    }
    if !pc.is_multiple_of(4) {
        return Err(IaotError::Pattern("xref address is unaligned".into()));
    }
    let imm26 = (instruction & 0x03ff_ffff) as i64;
    let signed = (imm26 << 38) >> 38;
    Ok((pc as i128 + (signed as i128) * 4) as u64)
}
pub fn find_bl_xrefs(data: &[u8], base: u64, target: u64) -> Vec<usize> {
    data.chunks_exact(4)
        .enumerate()
        .filter_map(|(i, b)| {
            let insn = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            (resolve_bl_target(insn, base + (i as u64) * 4).ok() == Some(target)).then_some(i * 4)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcard() {
        let p = parse("AA BB ?? CC").unwrap();
        assert_eq!(find(&[0xAA, 0xBB, 1, 0xCC], &p[0]), vec![0]);
    }
    #[test]
    fn alternatives() {
        assert_eq!(parse("AA || BB CC").unwrap().len(), 2);
    }
    #[test]
    fn bad() {
        assert!(parse("GG").is_err());
    }
    #[test]
    fn aligned_find_skips_unaligned_hits() {
        let p = parse("AA ?? CC").unwrap();
        let data = [0xAA, 0, 0xCC, 0, 0, 0xAA, 1, 0xCC, 0xAA, 2, 0xCC, 0];
        assert_eq!(find(&data, &p[0]), vec![0, 5, 8]);
        assert_eq!(find_aligned(&data, &p[0], 4), vec![0, 8]);
    }
    #[test]
    fn branch_targets_accept_b_and_bl_only() {
        // BL +8, B +8, B -4 (tail call backwards).
        assert_eq!(resolve_branch_target(0x9400_0002, 0x1000).unwrap(), 0x1008);
        assert_eq!(resolve_branch_target(0x1400_0002, 0x1000).unwrap(), 0x1008);
        assert_eq!(resolve_branch_target(0x17ff_ffff, 0x1000).unwrap(), 0xffc);
        // CBZ and RET are not branches with an imm26 target.
        assert!(resolve_branch_target(0xb400_0040, 0x1000).is_err());
        assert!(resolve_branch_target(0xd65f_03c0, 0x1000).is_err());
    }
    #[test]
    fn bl_xref() {
        assert_eq!(resolve_bl_target(0x9400_0002, 0x1000).unwrap(), 0x1008);
        assert_eq!(
            find_bl_xrefs(&0x9400_0002u32.to_le_bytes(), 0x1000, 0x1008),
            vec![0]
        );
    }
}
