//! 最小 XP3 写入器示例:把一组命名文件打包为 version 1 XP3 归档。
//!
//! 用法:
//! ```sh
//! cargo run -p yuzu-xp3 --example build_demo_xp3 -- <out.xp3> <name>=<path>...
//! ```
//! 示例:
//! ```sh
//! cargo run -p yuzu-xp3 --example build_demo_xp3 -- \
//!   web/data/game.xp3 "c01c.txt.scn=fixtures/c01c.txt.scn" "title.pimg=fixtures/title.pimg"
//! ```
//!
//! 生成的归档用原始(未压缩)段、adlr 键为 0,与 `Xp3Archive` 读取器往返兼容,
//! 也用于 web 演示「浏览器内解包」。

use std::fs;
use std::path::Path;

const MAGIC: &[u8; 11] = b"XP3\r\n\x20\x0a\x1a\x8b\x67\x01";
const HEADER_LEN: usize = 11 + 8 + 8; // 魔数 + index_offset + index_size

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("用法: build_demo_xp3 <out.xp3> <name>=<path>...");
        std::process::exit(2);
    }
    let out = &args[0];
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for a in &args[1..] {
        let (name, path) = a.split_once('=').expect("参数格式: <name>=<path>");
        let data = fs::read(path).unwrap_or_else(|e| panic!("读取 {path}: {e}"));
        files.push((name.to_string(), data));
    }
    write_xp3(out, &files);
    eprintln!(
        "✓ 已写入 {out}: {} 个文件,共 {} 字节",
        files.len(),
        files.iter().map(|(_, d)| d.len()).sum::<usize>()
    );
}

fn write_xp3(out: &str, files: &[(String, Vec<u8>)]) {
    // 数据段紧随文件头之后;每个文件原始存储。
    let mut data_offset = HEADER_LEN as u64;
    let mut data_section: Vec<u8> = Vec::new();
    let mut index: Vec<u8> = Vec::new();

    // 索引:先写"未压缩"标志 + 压缩后大小(占位,稍后回填)。
    index.push(0); // table_is_compressed
    let size_pos = index.len();
    index.extend_from_slice(&0u64.to_le_bytes()); // table_size_comp 占位

    for (name, data) in files {
        // "File" 条目
        index.extend_from_slice(b"File");
        let entry_size_pos = index.len();
        index.extend_from_slice(&0u64.to_le_bytes()); // entry_size 占位
        let chunk_start = index.len();

        // info 分块
        index.extend_from_slice(b"info");
        let info_data = info_chunk(name, data.len() as u64);
        index.extend_from_slice(&(info_data.len() as u64).to_le_bytes());
        index.extend_from_slice(&info_data);

        // segm 分块(单段,原始)
        index.extend_from_slice(b"segm");
        let mut segm = Vec::new();
        segm.extend_from_slice(&0u32.to_le_bytes()); // flags = 未压缩
        segm.extend_from_slice(&data_offset.to_le_bytes());
        segm.extend_from_slice(&(data.len() as u64).to_le_bytes()); // size_orig
        segm.extend_from_slice(&(data.len() as u64).to_le_bytes()); // size_comp
        index.extend_from_slice(&(segm.len() as u64).to_le_bytes());
        index.extend_from_slice(&segm);

        // adlr 分块(hash = 0)
        index.extend_from_slice(b"adlr");
        index.extend_from_slice(&4u64.to_le_bytes());
        index.extend_from_slice(&0u32.to_le_bytes());

        // 回填 entry_size
        let entry_size = (index.len() - chunk_start) as u64;
        index[entry_size_pos..entry_size_pos + 8].copy_from_slice(&entry_size.to_le_bytes());

        // 数据段
        data_section.extend_from_slice(data);
        data_offset += data.len() as u64;
    }

    // 回填表大小
    let table_size = (index.len() - 1 - 8) as u64; // 去掉标志 + 大小字段本身
    index[size_pos..size_pos + 8].copy_from_slice(&table_size.to_le_bytes());

    let index_offset = HEADER_LEN as u64 + data_section.len() as u64;
    let index_size = index.len() as u64;

    // 写文件
    let mut out_buf = Vec::new();
    out_buf.extend_from_slice(MAGIC);
    out_buf.extend_from_slice(&index_offset.to_le_bytes());
    out_buf.extend_from_slice(&index_size.to_le_bytes());
    out_buf.extend_from_slice(&data_section);
    out_buf.extend_from_slice(&index);
    if let Some(parent) = Path::new(out).parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(out, &out_buf).unwrap_or_else(|e| panic!("写入 {out}: {e}"));
}

fn info_chunk(name: &str, size: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes()); // flags
    out.extend_from_slice(&size.to_le_bytes()); // file_size_orig
    out.extend_from_slice(&size.to_le_bytes()); // file_size_comp
    let name16: Vec<u16> = name.encode_utf16().collect();
    out.extend_from_slice(&(name16.len() as u16).to_le_bytes());
    for u in name16 {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}
