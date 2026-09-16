//! Audited, fixed ARM64 cave templates. No user-provided instruction bytes are accepted.
use crate::{arm64, errors::IaotError, manifest::PatchAction};

pub struct CaveRequest<'a> {
    pub action: &'a PatchAction,
    pub cave_address: u64,
    pub return_address: u64,
    pub original_address: u64,
    pub displaced: &'a [u32],
    pub config_address: u64,
    pub flag_index: Option<u32>,
    pub callback_index: Option<u32>,
    pub replacement_address: Option<u64>,
}

fn put(out: &mut Vec<u8>, instruction: u32) {
    out.extend_from_slice(&instruction.to_le_bytes());
}
fn pc(req: &CaveRequest<'_>, out: &[u8]) -> u64 {
    req.cave_address + out.len() as u64
}
fn load_config(
    out: &mut Vec<u8>,
    req: &CaveRequest<'_>,
    rt: u8,
    offset: u16,
    is64: bool,
) -> Result<(), IaotError> {
    let now = pc(req, out);
    put(out, arm64::adrp(16, now, req.config_address)?);
    put(out, arm64::ldr_str_imm(rt, 16, offset, is64, true)?);
    Ok(())
}
fn branch_back(out: &mut Vec<u8>, req: &CaveRequest<'_>) -> Result<(), IaotError> {
    let now = pc(req, out);
    if let Ok(branch) = arm64::b(now, req.return_address) {
        put(out, branch);
    } else {
        for instruction in arm64::long_branch(now, req.return_address)? {
            put(out, instruction);
        }
    }
    Ok(())
}
fn normal(out: &mut Vec<u8>, req: &CaveRequest<'_>) -> Result<(), IaotError> {
    for (index, instruction) in req.displaced.iter().copied().enumerate() {
        let relocated = arm64::relocate(
            instruction,
            req.original_address + (index as u64) * 4,
            pc(req, out),
        )?;
        for instruction in relocated {
            put(out, instruction);
        }
    }
    branch_back(out, req)
}
fn flag_offset(index: u32) -> Result<u16, IaotError> {
    u16::try_from(72u32 + index * 4)
        .map_err(|_| IaotError::Unsupported("flag offset overflow".into()))
}
fn callback_offset(index: u32) -> Result<u16, IaotError> {
    u16::try_from(1096u32 + index * 8)
        .map_err(|_| IaotError::Unsupported("callback offset overflow".into()))
}

// Preserve caller-saved general registers, q0-q7, LR, and NZCV around observational callbacks.
fn save_gp(out: &mut Vec<u8>) -> Result<(), IaotError> {
    put(out, arm64::sub_imm(31, 31, 304, true)?);
    for pair in 0..9u8 {
        put(
            out,
            arm64::stp_ldp(pair * 2, pair * 2 + 1, 31, (pair as i16) * 16, false)?,
        );
    }
    put(out, arm64::stp_ldp(30, 31, 31, 144, false)?);
    for pair in 0..4u8 {
        put(
            out,
            arm64::q_stp_ldp(pair * 2, pair * 2 + 1, 31, 160 + pair as u16 * 32, false)?,
        );
    }
    put(out, arm64::mrs_nzcv(16)?);
    put(out, arm64::ldr_str_imm(16, 31, 288, true, false)?);
    Ok(())
}
fn restore_gp(out: &mut Vec<u8>) -> Result<(), IaotError> {
    put(out, arm64::ldr_str_imm(16, 31, 288, true, true)?);
    put(out, arm64::msr_nzcv(16)?);
    for pair in 0..4u8 {
        put(
            out,
            arm64::q_stp_ldp(pair * 2, pair * 2 + 1, 31, 160 + pair as u16 * 32, true)?,
        );
    }
    for pair in 0..9u8 {
        put(
            out,
            arm64::stp_ldp(pair * 2, pair * 2 + 1, 31, (pair as i16) * 16, true)?,
        );
    }
    put(out, arm64::stp_ldp(30, 31, 31, 144, true)?);
    put(out, arm64::add_imm(31, 31, 304, true)?);
    Ok(())
}
fn callback_call(out: &mut Vec<u8>, req: &CaveRequest<'_>, index: u32) -> Result<(), IaotError> {
    save_gp(out)?;
    load_config(out, req, 17, callback_offset(index)?, true)?;
    let skip_pc = pc(req, out);
    let skip_target = skip_pc + 8;
    put(out, arm64::cbz(17, skip_pc, skip_target, true)?);
    put(out, arm64::blr(17)?);
    restore_gp(out)
}
fn push_scratch(out: &mut Vec<u8>) -> Result<(), IaotError> {
    put(out, arm64::stp_pre(16, 17, 31, -16)?);
    Ok(())
}
fn pop_scratch(out: &mut Vec<u8>) -> Result<(), IaotError> {
    put(out, arm64::ldp_post(16, 17, 31, 16)?);
    Ok(())
}

pub fn build(req: &CaveRequest<'_>) -> Result<Vec<u8>, IaotError> {
    let mut out = Vec::new();
    match req.action {
        PatchAction::RetIfFlag { .. } => {
            push_scratch(&mut out)?;
            let index = req.flag_index.unwrap();
            load_config(&mut out, req, 17, flag_offset(index)?, false)?;
            let at = pc(req, &out);
            let placeholder = out.len();
            put(&mut out, 0);
            pop_scratch(&mut out)?;
            put(&mut out, arm64::ret());
            let normal_addr = pc(req, &out);
            pop_scratch(&mut out)?;
            normal(&mut out, req)?;
            out[placeholder..placeholder + 4]
                .copy_from_slice(&arm64::cbz(17, at, normal_addr, false)?.to_le_bytes());
        }
        PatchAction::ZeroRegisterIfFlag { register, .. } => {
            push_scratch(&mut out)?;
            let index = req.flag_index.unwrap();
            load_config(&mut out, req, 17, flag_offset(index)?, false)?;
            let at = pc(req, &out);
            let placeholder = out.len();
            put(&mut out, 0);
            pop_scratch(&mut out)?;
            put(&mut out, arm64::movz(*register, 0, 0, true)?);
            branch_back(&mut out, req)?;
            let normal_addr = pc(req, &out);
            pop_scratch(&mut out)?;
            normal(&mut out, req)?;
            out[placeholder..placeholder + 4]
                .copy_from_slice(&arm64::cbz(17, at, normal_addr, false)?.to_le_bytes());
        }
        PatchAction::CallbackIfFlag { .. } => {
            push_scratch(&mut out)?;
            let flag = req.flag_index.unwrap();
            load_config(&mut out, req, 17, flag_offset(flag)?, false)?;
            let at = pc(req, &out);
            let placeholder = out.len();
            put(&mut out, 0);
            pop_scratch(&mut out)?;
            callback_call(&mut out, req, req.callback_index.unwrap())?;
            normal(&mut out, req)?;
            let normal_addr = pc(req, &out);
            pop_scratch(&mut out)?;
            normal(&mut out, req)?;
            out[placeholder..placeholder + 4]
                .copy_from_slice(&arm64::cbz(17, at, normal_addr, false)?.to_le_bytes());
        }
        PatchAction::CallbackReturnZeroIfFlag { .. } => {
            push_scratch(&mut out)?;
            let flag = req.flag_index.unwrap();
            load_config(&mut out, req, 17, flag_offset(flag)?, false)?;
            let at = pc(req, &out);
            let placeholder = out.len();
            put(&mut out, 0);
            pop_scratch(&mut out)?;
            callback_call(&mut out, req, req.callback_index.unwrap())?;
            put(&mut out, arm64::movz(0, 0, 0, true)?);
            put(&mut out, arm64::ret());
            let normal_addr = pc(req, &out);
            pop_scratch(&mut out)?;
            normal(&mut out, req)?;
            out[placeholder..placeholder + 4]
                .copy_from_slice(&arm64::cbz(17, at, normal_addr, false)?.to_le_bytes());
        }
        PatchAction::CallbackAfter { .. } => {
            normal(&mut out, req)?;
            callback_call(&mut out, req, req.callback_index.unwrap())?;
            branch_back(&mut out, req)?;
        }
        PatchAction::BranchToCallback { .. } => {
            push_scratch(&mut out)?;
            load_config(
                &mut out,
                req,
                17,
                callback_offset(req.callback_index.unwrap())?,
                true,
            )?;
            let at = pc(req, &out);
            let placeholder = out.len();
            put(&mut out, 0);
            put(&mut out, arm64::mov(15, 17, true)?);
            pop_scratch(&mut out)?;
            put(&mut out, arm64::br(15)?);
            let normal_addr = pc(req, &out);
            pop_scratch(&mut out)?;
            normal(&mut out, req)?;
            out[placeholder..placeholder + 4]
                .copy_from_slice(&arm64::cbz(17, at, normal_addr, true)?.to_le_bytes());
        }
        PatchAction::BranchToReplacement { .. } => {
            let target = req.replacement_address.ok_or_else(|| {
                IaotError::Unsupported("replacement address is unresolved".into())
            })?;
            let now = pc(req, &out);
            put(&mut out, arm64::adrp(16, now, target)?);
            put(
                &mut out,
                arm64::add_imm(16, 16, (target & 0xfff) as u16, true)?,
            );
            put(&mut out, arm64::br(16)?);
        }
    }
    Ok(out)
}

pub fn build_original_trampoline(
    cave_address: u64,
    original_address: u64,
    displaced: &[u32],
) -> Result<Vec<u8>, IaotError> {
    let mut out = Vec::new();
    for (index, instruction) in displaced.iter().copied().enumerate() {
        let relocated = arm64::relocate(
            instruction,
            original_address + (index as u64) * 4,
            cave_address + out.len() as u64,
        )?;
        for instruction in relocated {
            put(&mut out, instruction);
        }
    }
    let branch_pc = cave_address + out.len() as u64;
    let return_address = original_address + (displaced.len() as u64) * 4;
    if let Ok(branch) = arm64::b(branch_pc, return_address) {
        put(&mut out, branch);
    } else {
        for instruction in arm64::long_branch(branch_pc, return_address)? {
            put(&mut out, instruction);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_template_encodes() {
        let actions = [
            PatchAction::RetIfFlag { flag: "f".into() },
            PatchAction::ZeroRegisterIfFlag {
                flag: "f".into(),
                register: 0,
            },
            PatchAction::CallbackIfFlag {
                flag: "f".into(),
                callback: Some("c".into()),
            },
            PatchAction::CallbackReturnZeroIfFlag {
                flag: "f".into(),
                callback: Some("c".into()),
            },
            PatchAction::CallbackAfter {
                callback: Some("c".into()),
            },
            PatchAction::BranchToCallback {
                callback: Some("c".into()),
            },
            PatchAction::BranchToReplacement { symbol: "r".into() },
        ];
        for action in &actions {
            let bytes = build(&CaveRequest {
                action,
                cave_address: 0x1_0000_8000,
                return_address: 0x1_0000_0404,
                original_address: 0x1_0000_0400,
                displaced: &[0xd503201f],
                config_address: 0x1_0000_4000,
                flag_index: Some(0),
                callback_index: Some(0),
                replacement_address: Some(0x1_0000_1000),
            })
            .unwrap();
            assert!(!bytes.is_empty());
            assert_eq!(bytes.len() % 4, 0);
            assert!(!bytes.chunks_exact(4).any(|x| x == [0, 0, 0, 0]));
        }
    }
    #[test]
    fn builds_callable_original() {
        let bytes = build_original_trampoline(0x1_0000_9000, 0x1_0000_0400, &[0xd503201f]).unwrap();
        assert_eq!(bytes.len(), 8);
    }
}
