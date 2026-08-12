//! KiriKiri XP3 加密方案(移植自 GARbro `KiriKiriCx.cs` / `YuzCrypt.cs`)。
//!
//! 所有方案接口见 [`Scheme`]。`decrypt(data, hash)` 的 `hash` 为条目的
//! `adlr` 校验键。

/// 加解密方案:对一段数据按其条目哈希键解密。
pub trait Scheme {
    fn decrypt(&self, data: &mut [u8], hash: u32);
}

/// 简易 XOR 方案族(arc_unpacker `xp3_archive_decoder_plugins.cc` 的 simple 插件)。
/// 各方案以 `decrypt(data, hash)` 的 `hash`(条目 adlr 键)为密钥输入。
#[derive(Debug, Clone, Copy)]
pub enum SimpleKind {
    /// 全字节 XOR 键低字节
    Xor,
    /// XOR `(键 + 1) ^ 0xFF`
    XorP1Neg,
    /// 偶数位 XOR 键,奇数位 XOR 索引
    XorMix,
    /// Dieselmine 分段 XOR/减
    Dieselmine,
    /// XOR `0xCD ^ 键`
    Moteyaba,
    /// XOR `0xCD`
    Kamiyaba,
    /// 从第 5 字节起 XOR `键 >> 12`
    Rebirth,
    /// 全字节 XOR `0x36`,另修正两个特定位
    Fsn,
}

#[derive(Debug, Clone, Copy)]
pub struct Simple(pub SimpleKind);

impl Scheme for Simple {
    fn decrypt(&self, data: &mut [u8], hash: u32) {
        match self.0 {
            SimpleKind::Xor => {
                let k = hash as u8;
                for b in data.iter_mut() {
                    *b ^= k;
                }
            }
            SimpleKind::XorP1Neg => {
                let k = (key_inc(hash) ^ 0xFF) as u8;
                for b in data.iter_mut() {
                    *b ^= k;
                }
            }
            SimpleKind::XorMix => {
                for (i, b) in data.iter_mut().enumerate() {
                    if i % 2 == 0 {
                        *b ^= hash as u8;
                    } else {
                        *b ^= i as u8;
                    }
                }
            }
            SimpleKind::Dieselmine => {
                let k = |n: u32| n.wrapping_mul(hash);
                let mut pos = 0usize;
                while pos < data.len() && pos < 0x7B {
                    data[pos] ^= k(21) as u8;
                    pos += 1;
                }
                while pos < data.len() && pos < 0xF6 {
                    data[pos] = data[pos].wrapping_sub(k(32) as u8);
                    pos += 1;
                }
                while pos < data.len() && pos < 0x171 {
                    data[pos] ^= k(43) as u8;
                    pos += 1;
                }
                while pos < data.len() {
                    data[pos] = data[pos].wrapping_sub(k(54) as u8);
                    pos += 1;
                }
            }
            SimpleKind::Moteyaba => {
                let k = (0xCDu32 ^ hash) as u8;
                for b in data.iter_mut() {
                    *b ^= k;
                }
            }
            SimpleKind::Kamiyaba => {
                for b in data.iter_mut() {
                    *b ^= 0xCD;
                }
            }
            SimpleKind::Rebirth => {
                let k = (hash >> 12) as u8;
                for b in data.iter_mut().skip(5) {
                    *b ^= k;
                }
            }
            SimpleKind::Fsn => {
                for b in data.iter_mut() {
                    *b ^= 0x36;
                }
                if data.len() > 0x2EA29 {
                    data[0x2EA29] ^= 3;
                }
                if data.len() > 0x13 {
                    data[0x13] ^= 1;
                }
            }
        }
    }
}

fn key_inc(key: u32) -> u32 {
    key.wrapping_add(1)
}

// =========================================================================
// CxScheme — SenrenCxCrypt 核心(CxEncryption + xcode 虚拟机)
// =========================================================================

/// SenrenCxCrypt 的组态:掩码 / 偏移 / 分支顺序 / 加密控制块。
///
/// `control_block` 为 0x400 个 u32(通常来自游戏的 TPM 插件);`prolog` /
/// `odd` / `even` 为该游戏的分支顺序。同一游戏的组态可从 GARbro 的
/// Xp3Options 或游戏 TPM 文件取得。
#[derive(Debug, Clone)]
pub struct CxScheme {
    pub mask: u32,
    pub offset: u32,
    pub prolog: [u8; 3],
    pub odd: [u8; 6],
    pub even: [u8; 8],
    pub control_block: Vec<u32>,
}

impl CxScheme {
    /// 从游戏 TPM 插件提取加密控制块(移植 GARbro `CxEncryption.Init`)。
    ///
    /// TPM 中存的是**取反后**的控制块:找到 ASCII 签名 `" Encryption control block"`
    /// 后,把其后的 0x400 个 u32 逐个取反,即为 [`CxScheme::control_block`]。
    /// `mask` / `offset` / `prolog` / `odd` / `even` 为按游戏的已知组态常量
    /// (GARbro 的 Xp3Options / arc_unpacker 插件参数)。
    pub fn from_tpm(
        tpm: &[u8],
        mask: u32,
        offset: u32,
        prolog: [u8; 3],
        odd: [u8; 6],
        even: [u8; 8],
    ) -> Option<Self> {
        const SIG: &[u8] = b" Encryption control block";
        const BLOCK_LEN: usize = 0x400;
        // GARbro 在距文件尾 0x1000 内停止扫描,且控制块按 4 字节对齐
        let scan_end = tpm.len().saturating_sub(0x1000) & !3;
        let mut pos = 0usize;
        while pos < scan_end {
            if tpm.len() >= pos + SIG.len() + BLOCK_LEN * 4 && tpm[pos..pos + SIG.len()] == *SIG {
                let mut control_block = Vec::with_capacity(BLOCK_LEN);
                let base = pos + SIG.len();
                for i in 0..BLOCK_LEN {
                    control_block.push(!le_u32(tpm, base + i * 4));
                }
                return Some(CxScheme {
                    mask,
                    offset,
                    prolog,
                    odd,
                    even,
                    control_block,
                });
            }
            pos += 4;
        }
        None
    }

    fn get_base_offset(&self, hash: u32) -> u32 {
        (hash & self.mask).wrapping_add(self.offset)
    }

    fn generate_program(&self, seed: u32) -> Result<CxProgram, String> {
        for stage in (1..=5).rev() {
            let mut program = CxProgram::new(seed, self.control_block.clone());
            if emit_code(&mut program, stage, self) {
                return Ok(program);
            }
        }
        Err("Overly large CxEncryption bytecode".into())
    }

    /// 执行 xcode 程序,返回 (ret1, ret2)。
    fn execute_xcode(&self, hash: u32) -> Result<(u32, u32), String> {
        let seed = hash & 0x7f;
        let program = self.generate_program(seed)?;
        let h = hash >> 7;
        let ret1 = program.execute(h)?;
        let ret2 = program.execute(!h)?;
        Ok((ret1, ret2))
    }

    /// 对一段数据(自文件偏移 `file_offset` 起)做 CxEncryption 解密。
    ///
    /// 偏移位于 base_offset 之前的部分用原键,之后用折叠键 `(key>>16)^key`。
    pub fn decrypt_chunk(&self, hash: u32, file_offset: u64, data: &mut [u8]) {
        let base = self.get_base_offset(hash) as u64;
        let mut offset = file_offset;
        if offset < base {
            let n = ((base - offset) as usize).min(data.len());
            self.decode(hash, offset, &mut data[..n]);
            offset += n as u64;
        }
        if offset < file_offset + data.len() as u64 {
            let key = (hash >> 16) ^ hash;
            let start = (offset - file_offset) as usize;
            self.decode(key, offset, &mut data[start..]);
        }
    }

    fn decode(&self, key: u32, file_offset: u64, data: &mut [u8]) {
        let (ret1, ret2) = match self.execute_xcode(key) {
            Ok(v) => v,
            Err(_) => return, // 组态损坏时无法解密,原样保留
        };
        let key1 = ret2 >> 16;
        let mut key2 = ret2 & 0xffff;
        let mut key3 = (ret1 & 0xff) as u8;
        if key1 == key2 {
            key2 = key2.wrapping_add(1);
        }
        if key3 == 0 {
            key3 = 1;
        }
        let xor_a = (ret1 >> 16) as u8;
        let xor_b = (ret1 >> 8) as u8;
        for (i, b) in data.iter_mut().enumerate() {
            let foff = file_offset + i as u64;
            if foff == key2 as u64 {
                *b ^= xor_a;
            }
            if foff == key1 as u64 {
                *b ^= xor_b;
            }
            *b ^= key3;
        }
    }
}

impl Scheme for CxScheme {
    fn decrypt(&self, data: &mut [u8], hash: u32) {
        self.decrypt_chunk(hash, 0, data);
    }
}

/// RiddleCxCrypt(YuzuSoft):前 8 字节按 adlr 哈希派生密钥 XOR,再走 CxScheme。
#[derive(Debug, Clone)]
pub struct Riddle(pub CxScheme);

impl Scheme for Riddle {
    fn decrypt(&self, data: &mut [u8], hash: u32) {
        riddle_first_bytes(hash, data);
        self.0.decrypt(data, hash);
    }
}

/// Riddle 方案的前 8 字节 XOR(自 `GetKeyFromHash` 派生 64 位密钥)。
pub fn riddle_first_bytes(hash: u32, data: &mut [u8]) {
    let n = data.len().min(8);
    let mut key = get_key_from_hash(hash);
    for b in &mut data[..n] {
        *b ^= key as u8;
        key >>= 8;
    }
}

fn get_key_from_hash(hash: u32) -> u64 {
    let lo = hash ^ 0x5555_5555;
    let mut hi = (hash << 13) ^ hash;
    hi ^= hi >> 17;
    hi ^= (hi << 5) ^ 0xAAAA_AAAA;
    ((hi as u64) << 32) | (lo as u64)
}

// ---------------- CxProgram(xcode 虚拟机) ----------------

const BC_NOP: u32 = 0;
const BC_RETN: u32 = 1;
const BC_MOV_EDI_ARG: u32 = 2;
const BC_PUSH_EBX: u32 = 3;
const BC_POP_EBX: u32 = 4;
const BC_PUSH_ECX: u32 = 5;
const BC_POP_ECX: u32 = 6;
const BC_MOV_EAX_EBX: u32 = 7;
const BC_MOV_EBX_EAX: u32 = 8;
const BC_MOV_ECX_EBX: u32 = 9;
const BC_MOV_EAX_EDI: u32 = 11;
const BC_MOV_EAX_INDIRECT: u32 = 12;
const BC_ADD_EAX_EBX: u32 = 13;
const BC_SUB_EAX_EBX: u32 = 14;
const BC_IMUL_EAX_EBX: u32 = 15;
const BC_AND_ECX_0F: u32 = 16;
const BC_SHR_EBX_1: u32 = 17;
const BC_SHL_EAX_1: u32 = 18;
const BC_SHR_EAX_CL: u32 = 19;
const BC_SHL_EAX_CL: u32 = 20;
const BC_OR_EAX_EBX: u32 = 21;
const BC_NOT_EAX: u32 = 22;
const BC_NEG_EAX: u32 = 23;
const BC_DEC_EAX: u32 = 24;
const BC_INC_EAX: u32 = 25;
const BC_IMMED: u32 = 0x100;
const BC_MOV_EAX_IMMED: u32 = 0x101;
const BC_AND_EBX_IMMED: u32 = 0x102;
const BC_AND_EAX_IMMED: u32 = 0x103;
const BC_XOR_EAX_IMMED: u32 = 0x104;
const BC_ADD_EAX_IMMED: u32 = 0x105;
const BC_SUB_EAX_IMMED: u32 = 0x106;

const LENGTH_LIMIT: usize = 0x80;

struct CxProgram {
    code: Vec<u32>,
    control_block: Vec<u32>,
    rng: u32,
    length: usize,
}

impl CxProgram {
    fn new(seed: u32, control_block: Vec<u32>) -> Self {
        CxProgram {
            code: Vec::new(),
            control_block,
            rng: seed,
            length: 0,
        }
    }

    fn execute(&self, hash: u32) -> Result<u32, String> {
        let mut eax = 0u32;
        let mut ebx = 0u32;
        let mut ecx = 0u32;
        let mut edi = 0u32;
        let mut stack: Vec<u32> = Vec::new();
        let mut iter = self.code.iter();
        let mut immed = 0u32;
        while let Some(&bc) = iter.next() {
            if bc & BC_IMMED == BC_IMMED {
                immed = *iter.next().ok_or("Incomplete IMMED bytecode")?;
            }
            match bc {
                BC_NOP => {}
                BC_IMMED => {}
                BC_MOV_EDI_ARG => edi = hash,
                BC_PUSH_EBX => stack.push(ebx),
                BC_POP_EBX => ebx = stack.pop().ok_or("Stack underflow")?,
                BC_PUSH_ECX => stack.push(ecx),
                BC_POP_ECX => ecx = stack.pop().ok_or("Stack underflow")?,
                BC_MOV_EBX_EAX => ebx = eax,
                BC_MOV_EAX_EDI => eax = edi,
                BC_MOV_ECX_EBX => ecx = ebx,
                BC_MOV_EAX_EBX => eax = ebx,
                BC_AND_ECX_0F => ecx &= 0x0f,
                BC_SHR_EBX_1 => ebx >>= 1,
                BC_SHL_EAX_1 => eax <<= 1,
                BC_SHR_EAX_CL => eax >>= ecx & 0x1f,
                BC_SHL_EAX_CL => eax <<= ecx & 0x1f,
                BC_OR_EAX_EBX => eax |= ebx,
                BC_NOT_EAX => eax = !eax,
                BC_NEG_EAX => eax = eax.wrapping_neg(),
                BC_DEC_EAX => eax = eax.wrapping_sub(1),
                BC_INC_EAX => eax = eax.wrapping_add(1),
                BC_ADD_EAX_EBX => eax = eax.wrapping_add(ebx),
                BC_SUB_EAX_EBX => eax = eax.wrapping_sub(ebx),
                BC_IMUL_EAX_EBX => eax = eax.wrapping_mul(ebx),
                BC_ADD_EAX_IMMED => eax = eax.wrapping_add(immed),
                BC_SUB_EAX_IMMED => eax = eax.wrapping_sub(immed),
                BC_AND_EBX_IMMED => ebx &= immed,
                BC_AND_EAX_IMMED => eax &= immed,
                BC_XOR_EAX_IMMED => eax ^= immed,
                BC_MOV_EAX_IMMED => eax = immed,
                BC_MOV_EAX_INDIRECT => {
                    let idx = eax as usize;
                    let v = self
                        .control_block
                        .get(idx)
                        .ok_or("Index out of bounds in CxEncryption program")?;
                    eax = !v;
                }
                BC_RETN => {
                    if !stack.is_empty() {
                        return Err("Imbalanced stack in CxEncryption program".into());
                    }
                    return Ok(eax);
                }
                other => return Err(format!("Invalid bytecode {other} in CxEncryption program")),
            }
        }
        Err("CxEncryption program without RETN bytecode".into())
    }

    fn get_random(&mut self) -> u32 {
        let seed = self.rng;
        self.rng = 1_103_515_245u32.wrapping_mul(seed).wrapping_add(12345);
        self.rng ^ (seed << 16) ^ (seed >> 16)
    }

    fn emit_nop(&mut self, count: usize) -> bool {
        if self.length + count > LENGTH_LIMIT {
            return false;
        }
        self.length += count;
        true
    }

    fn emit(&mut self, code: u32, length: usize) -> bool {
        if self.length + length > LENGTH_LIMIT {
            return false;
        }
        self.length += length;
        self.code.push(code);
        true
    }

    fn emit_u32(&mut self, x: u32) -> bool {
        if self.length + 4 > LENGTH_LIMIT {
            return false;
        }
        self.length += 4;
        self.code.push(x);
        true
    }

    fn emit_random(&mut self) -> bool {
        let r = self.get_random();
        self.emit_u32(r)
    }
}

/// 生成完整程序:`NOP×5` + `MOV EDI,ARG` + 主体 + `NOP×5` + `RETN`。
fn emit_code(program: &mut CxProgram, stage: i32, scheme: &CxScheme) -> bool {
    program.emit_nop(5)
        && program.emit(BC_MOV_EDI_ARG, 4)
        && emit_body(program, stage, scheme)
        && program.emit_nop(5)
        && program.emit(BC_RETN, 1)
}

fn emit_body(program: &mut CxProgram, stage: i32, scheme: &CxScheme) -> bool {
    if stage == 1 {
        return emit_prolog(program, scheme);
    }
    if !program.emit(BC_PUSH_EBX, 1) {
        return false;
    }
    if program.get_random() & 1 != 0 {
        if !emit_body(program, stage - 1, scheme) {
            return false;
        }
    } else if !emit_body2(program, stage - 1, scheme) {
        return false;
    }
    if !program.emit(BC_MOV_EBX_EAX, 2) {
        return false;
    }
    if program.get_random() & 1 != 0 {
        if !emit_body(program, stage - 1, scheme) {
            return false;
        }
    } else if !emit_body2(program, stage - 1, scheme) {
        return false;
    }
    emit_odd_branch(program, scheme) && program.emit(BC_POP_EBX, 1)
}

fn emit_body2(program: &mut CxProgram, stage: i32, scheme: &CxScheme) -> bool {
    if stage == 1 {
        return emit_prolog(program, scheme);
    }
    let rc = if program.get_random() & 1 != 0 {
        emit_body(program, stage - 1, scheme)
    } else {
        emit_body2(program, stage - 1, scheme)
    };
    rc && emit_even_branch(program, scheme)
}

fn emit_prolog(program: &mut CxProgram, scheme: &CxScheme) -> bool {
    match scheme.prolog[(program.get_random() % 3) as usize] {
        2 => {
            let idx = program.get_random() & 0x3ff;
            program.emit_nop(5)
                && program.emit(BC_MOV_EAX_IMMED, 2)
                && program.emit_u32(idx)
                && program.emit(BC_MOV_EAX_INDIRECT, 0)
        }
        1 => program.emit(BC_MOV_EAX_EDI, 2),
        _ => program.emit(BC_MOV_EAX_IMMED, 1) && program.emit_random(),
    }
}

fn emit_even_branch(program: &mut CxProgram, scheme: &CxScheme) -> bool {
    match scheme.even[(program.get_random() & 7) as usize] {
        0 => program.emit(BC_NOT_EAX, 2),
        1 => program.emit(BC_DEC_EAX, 1),
        2 => program.emit(BC_NEG_EAX, 2),
        3 => program.emit(BC_INC_EAX, 1),
        4 => {
            program.emit_nop(5)
                && program.emit(BC_AND_EAX_IMMED, 1)
                && program.emit_u32(0x3ff)
                && program.emit(BC_MOV_EAX_INDIRECT, 3)
        }
        5 => {
            program.emit(BC_PUSH_EBX, 1)
                && program.emit(BC_MOV_EBX_EAX, 2)
                && program.emit(BC_AND_EBX_IMMED, 2)
                && program.emit_u32(0xAAAA_AAAA)
                && program.emit(BC_AND_EAX_IMMED, 1)
                && program.emit_u32(0x5555_5555)
                && program.emit(BC_SHR_EBX_1, 2)
                && program.emit(BC_SHL_EAX_1, 2)
                && program.emit(BC_OR_EAX_EBX, 2)
                && program.emit(BC_POP_EBX, 1)
        }
        6 => program.emit(BC_XOR_EAX_IMMED, 1) && program.emit_random(),
        _ => {
            let op = if program.get_random() & 1 != 0 {
                BC_ADD_EAX_IMMED
            } else {
                BC_SUB_EAX_IMMED
            };
            program.emit(op, 1) && program.emit_random()
        }
    }
}

fn emit_odd_branch(program: &mut CxProgram, scheme: &CxScheme) -> bool {
    match scheme.odd[(program.get_random() % 6) as usize] {
        0 => {
            program.emit(BC_PUSH_ECX, 1)
                && program.emit(BC_MOV_ECX_EBX, 2)
                && program.emit(BC_AND_ECX_0F, 3)
                && program.emit(BC_SHR_EAX_CL, 2)
                && program.emit(BC_POP_ECX, 1)
        }
        1 => {
            program.emit(BC_PUSH_ECX, 1)
                && program.emit(BC_MOV_ECX_EBX, 2)
                && program.emit(BC_AND_ECX_0F, 3)
                && program.emit(BC_SHL_EAX_CL, 2)
                && program.emit(BC_POP_ECX, 1)
        }
        2 => program.emit(BC_ADD_EAX_EBX, 2),
        3 => program.emit(BC_NEG_EAX, 2) && program.emit(BC_ADD_EAX_EBX, 2),
        4 => program.emit(BC_IMUL_EAX_EBX, 3),
        _ => program.emit(BC_SUB_EAX_EBX, 2),
    }
}

// =========================================================================
// YuzDecryptor / NanaDecryptor — 文件名列表解密(Senren / Nana / Riddle)
// =========================================================================

/// SenrenCxCrypt 系列的文件名列表解密器(ARX 状态机)。
///
/// `key1` 为 8 个 u32(通常取自控制块),`key2` 为 4 个 u32;
/// `seed1` / `seed2` 为随机种子。输出为 XOR 密钥流。
#[derive(Debug, Clone)]
pub struct YuzDecryptor {
    state: [u8; 64],
}

impl YuzDecryptor {
    pub fn new(key1: &[u32], key2: &[u32], seed1: u32, seed2: u32) -> Self {
        let mut state = [0u8; 64];
        for (i, v) in key2.iter().take(4).enumerate() {
            state[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in key1.iter().take(8).enumerate() {
            state[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        state[48..52].copy_from_slice(&(!0u32).to_le_bytes());
        state[52..56].copy_from_slice(&(!0u32).to_le_bytes());
        state[56..60].copy_from_slice(&(!seed1).to_le_bytes());
        state[60..64].copy_from_slice(&(!seed2).to_le_bytes());
        YuzDecryptor { state }
    }

    /// 对数据(通常为文件名列表)做 XOR 解密。
    pub fn decrypt(&self, data: &mut [u8]) {
        let mut state1 = [0u8; 64];
        let mut state2 = [0u8; 64];
        let mut offset = 0u64;
        let mut i = 0usize;
        let mut length = data.len();
        while length > 0 {
            state1.copy_from_slice(&self.state);
            state1[48..56].copy_from_slice(&(!offset).to_le_bytes());
            transform_state(&state1, &mut state2);
            let count = 0x40.min(length);
            for j in 0..count {
                data[i + j] ^= state2[j];
            }
            i += count;
            length -= count;
            offset = offset.wrapping_add(1);
        }
    }
}

/// YuzDecryptor 的单块状态变换(16×u32 ARX)。
// 移植自 GARbro YuzDecryptor.TransformState(索引式循环以对齐参考实现)。
#[allow(clippy::needless_range_loop)]
fn transform_state(state1: &[u8; 64], target: &mut [u8; 64]) {
    let mut tmp = [0u32; 16];
    for i in 0..16 {
        tmp[i] = !le_u32(state1, i * 4);
    }
    // 4 轮(对应 GARbro 的 length=8 → ((8-1)>>1)+1 = 4 次迭代)
    for _ in 0..4 {
        let mut t1 = tmp[4].wrapping_add(tmp[0]);
        let mut t2 = rotl(t1 ^ tmp[12], 16);
        let mut t3 = t2.wrapping_add(tmp[8]);
        let mut t4 = rotl(tmp[4] ^ t3, 12);
        let mut t5 = t4.wrapping_add(t1);
        let mut t6 = rotl(t5 ^ t2, 8);
        tmp[12] = t6;
        t6 = t6.wrapping_add(t3);
        tmp[4] = rotl(t4 ^ t6, 7);
        t4 = rotl(tmp[5].wrapping_add(tmp[1]) ^ tmp[13], 16);
        t3 = rotl(tmp[5] ^ t4.wrapping_add(tmp[9]), 12);
        t2 = t3.wrapping_add(tmp[5]).wrapping_add(tmp[1]);
        tmp[13] = rotl(t2 ^ t4, 8);
        tmp[9] = tmp[9].wrapping_add(tmp[13]).wrapping_add(t4);
        tmp[5] = rotl(t3 ^ tmp[9], 7);
        t4 = rotl(tmp[6].wrapping_add(tmp[2]) ^ tmp[14], 16);
        tmp[10] = tmp[10].wrapping_add(t4);
        t1 = rotl(tmp[6] ^ tmp[10], 12);
        t3 = t1.wrapping_add(tmp[6]).wrapping_add(tmp[2]);
        tmp[14] = rotl(t3 ^ t4, 8);
        tmp[6] = rotl(t1 ^ tmp[14].wrapping_add(tmp[10]), 7);
        tmp[10] = tmp[10].wrapping_add(tmp[14]);
        t4 = tmp[7].wrapping_add(tmp[3]) ^ tmp[15];
        tmp[3] = tmp[3].wrapping_add(tmp[7]);
        t4 = rotl(t4, 16);
        tmp[11] = tmp[11].wrapping_add(t4);
        t1 = rotl(tmp[7] ^ tmp[11], 12);
        t4 ^= t1.wrapping_add(tmp[3]);
        tmp[3] = tmp[3].wrapping_add(t1);
        t4 = rotl(t4, 8);
        tmp[11] = tmp[11].wrapping_add(t4);
        t1 = rotl(t1 ^ tmp[11], 7);
        t5 = t5.wrapping_add(tmp[5]);
        t2 = t2.wrapping_add(tmp[6]);
        t4 = rotl(t5 ^ t4, 16);
        tmp[10] = tmp[10].wrapping_add(t4);
        tmp[5] = rotl(tmp[5] ^ tmp[10], 12);
        tmp[0] = tmp[5].wrapping_add(t5);
        t4 = rotl(tmp[0] ^ t4, 8);
        tmp[15] = t4;
        tmp[10] = tmp[10].wrapping_add(t4);
        tmp[5] = rotl(tmp[5] ^ tmp[10], 7);
        tmp[12] = rotl(tmp[12] ^ t2, 16);
        tmp[11] = tmp[11].wrapping_add(tmp[12]);
        t4 = rotl(tmp[11] ^ tmp[6], 12);
        tmp[1] = t4.wrapping_add(t2);
        tmp[12] = rotl(tmp[12] ^ tmp[1], 8);
        tmp[11] = tmp[11].wrapping_add(tmp[12]);
        tmp[6] = rotl(t4 ^ tmp[11], 7);
        t3 = t3.wrapping_add(t1);
        t4 = rotl(tmp[13] ^ t3, 16);
        t2 = t4.wrapping_add(t6);
        t1 = rotl(t2 ^ t1, 12);
        tmp[2] = t1.wrapping_add(t3);
        tmp[13] = rotl(t4 ^ tmp[2], 8);
        tmp[8] = tmp[13].wrapping_add(t2);
        tmp[7] = rotl(tmp[8] ^ t1, 7);
        t6 = rotl(tmp[14] ^ tmp[4].wrapping_add(tmp[3]), 16);
        t1 = rotl(tmp[4] ^ t6.wrapping_add(tmp[9]), 12);
        tmp[3] = tmp[3].wrapping_add(t1).wrapping_add(tmp[4]);
        t3 = rotl(t6 ^ tmp[3], 8);
        tmp[9] = tmp[9].wrapping_add(t3).wrapping_add(t6);
        tmp[4] = rotl(t1 ^ tmp[9], 7);
        tmp[14] = t3;
    }
    let mut pos = 0;
    for i in 0..16 {
        let x = tmp[i].wrapping_add(!le_u32(state1, pos));
        target[pos..pos + 4].copy_from_slice(&x.to_le_bytes());
        pos += 4;
    }
}

/// NanaDecryptor(Cabbage / など):LCG 扩展密钥流。
#[derive(Debug, Clone)]
pub struct NanaDecryptor {
    state: [u32; 27],
    seed: u64,
}

impl NanaDecryptor {
    pub fn new(key: &[u32], seed1: u32, seed2: u32) -> Self {
        let mut state = [0u32; 27];
        let mut s = [0u32; 3];
        let mut k = key.first().copied().unwrap_or(0);
        s[0] = key.get(1).copied().unwrap_or(0);
        s[1] = key.get(2).copied().unwrap_or(0);
        s[2] = key.get(3).copied().unwrap_or(0);
        state[0] = k;
        for i in 0u32..26 {
            let src = (i % 3) as usize;
            let m = rotr(s[src], 8);
            let n = i ^ k.wrapping_add(m);
            k = n ^ rotl(k, 3);
            state[(i + 1) as usize] = k;
            s[src] = n;
        }
        NanaDecryptor {
            state,
            seed: ((seed2 as u64) << 32) | (seed1 as u64),
        }
    }

    pub fn decrypt(&self, data: &mut [u8]) {
        let mut offset = 0u64;
        let mut i = 0usize;
        let mut length = data.len();
        while length > 0 {
            let mut key = offset.wrapping_add(1) ^ self.seed;
            key = self.transform_key(key);
            let count = 8.min(length);
            for _ in 0..count {
                data[i] ^= key as u8;
                key >>= 8;
                i += 1;
            }
            length -= count;
            offset = offset.wrapping_add(1);
        }
    }

    fn transform_key(&self, key: u64) -> u64 {
        let mut lo = key as u32;
        let mut hi = (key >> 32) as u32;
        for i in 0..27 {
            hi = rotr(hi, 8).wrapping_add(lo);
            hi ^= self.state[i];
            lo = rotl(lo, 3) ^ hi;
        }
        ((hi as u64) << 32) | (lo as u64)
    }
}

// ---------------- 小工具 ----------------

fn rotl(x: u32, n: u32) -> u32 {
    x.rotate_left(n)
}

fn rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}

fn le_u32(b: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scheme() -> CxScheme {
        // 参考控制块:0..0x400 顺序值 + 全序分支
        CxScheme {
            mask: 0x7FF,
            offset: 0x1000,
            prolog: [0, 1, 2],
            odd: [0, 1, 2, 3, 4, 5],
            even: [0, 1, 2, 3, 4, 5, 6, 7],
            control_block: (0..0x400).collect(),
        }
    }

    #[test]
    fn cx_program_rng_matches_lcg_reference() {
        // 与 GARbro GetRandom / arc_unpacker rand() 对齐的 LCG 序列
        let mut p = CxProgram::new(0, vec![]);
        assert_eq!(p.get_random(), 0x3039); // 12345
        assert_eq!(p.get_random(), 0xe3e5167e);
        assert_eq!(p.get_random(), 0xb17af403);
        assert_eq!(p.get_random(), 0xf1babb28);
    }

    #[test]
    fn cx_program_generates_for_all_seeds() {
        let scheme = test_scheme();
        for seed in 0..128u32 {
            let program = scheme.generate_program(seed).expect("程序生成失败");
            let a = program.execute(0xDEADBEEF).expect("执行失败");
            let b = program.execute(0xDEADBEEF).expect("执行失败");
            assert_eq!(a, b, "同种子同参数应确定性");
        }
    }

    #[test]
    fn cx_scheme_decrypt_is_self_inverse_and_changes_data() {
        let scheme = test_scheme();
        let mut data: Vec<u8> = (0..64u8).collect();
        let original = data.clone();
        let hash = 0x1234_5678;
        scheme.decrypt(&mut data, hash);
        assert_ne!(data, original, "解密应改变数据");
        scheme.decrypt(&mut data, hash);
        assert_eq!(data, original, "两次解密应还原(XOR 自逆)");
    }

    #[test]
    fn riddle_scheme_is_self_inverse() {
        let scheme = Riddle(test_scheme());
        let mut data: Vec<u8> = (0..64u8).collect();
        let original = data.clone();
        let hash = 0xDEAD_BEEF;
        scheme.decrypt(&mut data, hash);
        scheme.decrypt(&mut data, hash);
        assert_eq!(data, original);
    }

    #[test]
    fn riddle_first_bytes_known_value() {
        // hash=0 → key = 0xAAAAAAAA55555555
        let mut data = [0u8; 8];
        riddle_first_bytes(0, &mut data);
        assert_eq!(data, [0x55, 0x55, 0x55, 0x55, 0xAA, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    fn simple_kamiyaba() {
        let mut data = [0u8, 1, 2, 0xFF];
        Simple(SimpleKind::Kamiyaba).decrypt(&mut data, 0);
        assert_eq!(data, [0xCD, 0xCC, 0xCF, 0x32]);
    }

    #[test]
    fn simple_xor() {
        let mut data = [0u8, 0x11, 0xFF];
        Simple(SimpleKind::Xor).decrypt(&mut data, 0x1234_5678);
        assert_eq!(data, [0x78, 0x69, 0x87]);
    }

    #[test]
    fn simple_xor_p1_neg() {
        let mut data = [0u8];
        Simple(SimpleKind::XorP1Neg).decrypt(&mut data, 0);
        assert_eq!(data, [0xFE]); // (0+1) ^ 0xFF
    }

    #[test]
    fn simple_moteyaba() {
        let mut data = [0u8, 0x11];
        Simple(SimpleKind::Moteyaba).decrypt(&mut data, 0);
        assert_eq!(data, [0xCD, 0xDC]); // 0x11 ^ 0xCD
    }

    #[test]
    fn simple_fsn() {
        let mut data = [0u8; 20];
        Simple(SimpleKind::Fsn).decrypt(&mut data, 0);
        assert_eq!(data[0], 0x36);
        assert_eq!(data[0x13], 0x37); // 0x36 ^ 1
        assert_eq!(data[5], 0x36);
    }

    #[test]
    fn simple_xor_mix_odd_positions_use_index() {
        let mut data = [0u8; 4];
        Simple(SimpleKind::XorMix).decrypt(&mut data, 0x5);
        assert_eq!(data[0], 0x05);
        assert_eq!(data[1], 0x01); // index 1
        assert_eq!(data[2], 0x05);
        assert_eq!(data[3], 0x03);
    }

    #[test]
    fn yuz_decryptor_deterministic_roundtrip() {
        let key1 = (0..8).map(|i| 0x1020_3040 + i).collect::<Vec<_>>();
        let key2 = vec![0x0F0F_0F0F; 4];
        let d = YuzDecryptor::new(&key1, &key2, 0x1111_2222, 0x3333_4444);
        let mut a = vec![0x41u8; 256];
        let b = a.clone();
        d.decrypt(&mut a);
        assert_ne!(a, b, "应改变数据");
        let mut c = a.clone();
        d.decrypt(&mut c);
        assert_eq!(c, b, "两次解密应还原");
        // 确定性
        let mut e = vec![0x41u8; 256];
        d.decrypt(&mut e);
        assert_eq!(e, a);
    }

    #[test]
    fn yuz_decryptor_different_keys_differ() {
        let key1 = (0..8).map(|i| 0x1020_3040 + i).collect::<Vec<_>>();
        let key2 = vec![0x0F0F_0F0F; 4];
        let d1 = YuzDecryptor::new(&key1, &key2, 0x1111_2222, 0x3333_4444);
        let d2 = YuzDecryptor::new(&key1, &key2, 0xAAAA_BBBB, 0x3333_4444);
        let mut a = vec![0u8; 128];
        let mut b = vec![0u8; 128];
        d1.decrypt(&mut a);
        d2.decrypt(&mut b);
        assert_ne!(a, b);
    }

    #[test]
    fn nana_decryptor_deterministic_roundtrip() {
        let key = vec![0x1234_5678, 0x9ABC_DEF0, 0x1122_3344, 0x5566_7788];
        let d = NanaDecryptor::new(&key, 0x0102_0304, 0x0506_0708);
        let mut a = vec![0x5Au8; 100];
        let b = a.clone();
        d.decrypt(&mut a);
        assert_ne!(a, b);
        let mut c = a.clone();
        d.decrypt(&mut c);
        assert_eq!(c, b);
        let mut e = vec![0x5Au8; 100];
        d.decrypt(&mut e);
        assert_eq!(e, a);
    }

    #[test]
    fn cx_scheme_extracts_control_block_from_tpm() {
        // 合成 TPM:前置填充 + 签名 + 0x400 个已知 u32(取反存储)
        const SIG: &[u8] = b" Encryption control block";
        let mut tpm: Vec<u8> = (0..=255u8).cycle().take(0x400).collect(); // 4 字节对齐填充
        tpm.extend_from_slice(SIG);
        for i in 0..0x400usize {
            let v = (0x1000 + i) as u32;
            tpm.extend_from_slice(&(!v).to_le_bytes());
        }
        tpm.extend_from_slice(&[0u8; 0x2000]); // 尾部(验证扫描在 0x1000 边界内停止也够长)

        let scheme = CxScheme::from_tpm(
            &tpm,
            0x7FF,
            0x1000,
            [0, 1, 2],
            [0, 1, 2, 3, 4, 5],
            [0, 1, 2, 3, 4, 5, 6, 7],
        )
        .expect("应提取到控制块");
        assert_eq!(scheme.control_block.len(), 0x400);
        // TPM 存 `!v`,提取时按 GARbro 取反一次 → 得回 `v`
        assert_eq!(scheme.control_block[0], 0x1000);
        assert_eq!(scheme.control_block[0x3FF], 0x13FF);

        // 提取出的组态应能正常生成程序(128 个种子均可)
        for seed in 0..128u32 {
            scheme.generate_program(seed).expect("程序生成失败");
        }
    }

    #[test]
    fn cx_scheme_from_tpm_missing_signature_returns_none() {
        assert!(CxScheme::from_tpm(
            &[0xAA; 0x3000],
            0,
            0,
            [0, 1, 2],
            [0, 1, 2, 3, 4, 5],
            [0, 1, 2, 3, 4, 5, 6, 7]
        )
        .is_none());
    }
}
