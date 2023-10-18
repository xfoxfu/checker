#![windows_subsystem = "windows"]

mod config;
mod render;

use anyhow::Result;
use config::Contestant;
use std::fs::{read_dir, File};
use std::io::Read;
use std::path::Path;

fn try_crc32<P: AsRef<Path>>(path: P) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| format!("无法读取文件内容:{}", e))?;
    let mut s = Vec::new();
    f.read_to_end(&mut s)
        .map_err(|e| format!("无法读取文件内容:{}", e))?;
    Ok(format!("{:x}", md5::compute(&s)))
}

/// This function is used just as its name tells. See Rust [`Result::into_ok_or_err`].
fn result_into_ok_or_err<T>(r: Result<T, T>) -> T {
    match r {
        Ok(v) => v,
        Err(v) => v,
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CSPError {
    #[error("错误 1, checker.cfg.json 不存在, 请联系监考员.\n({0})")]
    ConfigNotExist(#[source] std::io::Error),
    #[error("错误 2, checker.cfg.json 无法解析, 请联系监考员.\n({0})")]
    ConfigCannotParse(#[source] serde_json::Error),
    #[error("错误 2，checker.cfg.json 配置的根目录或其中文件无法访问，请联系监考员.\n({0})")]
    RootDirAccessFail(#[source] std::io::Error),
    #[error("错误 3, 没有找到有效的选手目录. 请阅读考生须知.")]
    NoValidContestantDir,
    #[error("错误 4, 找到多个选手目录.\n{0}")]
    MultipleContestantDir(String),
    #[error("无法解析 CSV 文件")]
    FailedToLoadCsv,
    #[error("程序内部错误，请联系监考员.\n({0})")]
    Unknown(
        #[from]
        #[source]
        anyhow::Error,
    ),
}

fn build_message(messages: &mut Vec<(String, Color)>) -> Result<()> {
    let cfg_file = if let Some(d) = std::env::args().nth(1) {
        File::open(Path::new(&d).join("checker.cfg.json"))
    } else {
        File::open("checker.cfg.json")
    };
    let cfg_file = cfg_file.map_err(CSPError::ConfigNotExist)?;
    let mut cfg: Contestant =
        serde_json::from_reader(cfg_file).map_err(CSPError::ConfigCannotParse)?;

    let mut valid_folders = Vec::new();

    for dir in read_dir(&cfg.root_path).map_err(CSPError::RootDirAccessFail)? {
        let dir = dir.map_err(CSPError::RootDirAccessFail)?;
        if dir.file_type()?.is_dir() && cfg.regex.is_match(dir.file_name().to_str().unwrap()) {
            valid_folders.push(dir.path());
        }
    }

    if valid_folders.is_empty() {
        Err(CSPError::NoValidContestantDir)?
    }

    if valid_folders.len() > 1 {
        Err(CSPError::MultipleContestantDir(
            valid_folders
                .iter()
                .map(|f| format!("    {:?}", f))
                .collect::<Vec<_>>()
                .join("\n"),
        ))?
    }

    let valid_folder_name = valid_folders.into_iter().next().unwrap();
    let user_directory = valid_folder_name;
    let student_id_found = Path::new(&user_directory)
        .strip_prefix(&cfg.root_path)?
        .to_str()
        .unwrap();

    messages.push((
        format!(
            "找到选手目录： {}, 请确认是否与准考证号一致.",
            student_id_found
        ),
        Color::Yellow,
    ));

    for dir1 in read_dir(&user_directory)? {
        let dir1 = dir1?;
        if !dir1.file_type()?.is_dir() {
            continue;
        }
        for dir2 in read_dir(dir1.path())? {
            let dir2 = dir2?;
            if !dir2.file_type()?.is_file() {
                continue;
            }
            for prob in cfg.problems.iter_mut() {
                if prob
                    .regex
                    .is_match(dir2.path().strip_prefix(&user_directory)?.to_str().unwrap())
                {
                    let filepath = dir2.path().to_str().unwrap().to_string();
                    prob.existing_files.push(filepath.clone());
                    if let Ok(meta) = dir2.metadata() {
                        if let Ok(modi) = meta.modified() {
                            prob.existing_files_date.insert(filepath, modi.into());
                        }
                    }
                }
            }
        }
    }

    for prob in cfg.problems.iter() {
        messages.push((format!("题目 {}: ", prob.name), Color::Black));
        if prob.existing_files.is_empty() {
            messages.push((format!("    未找到源代码文件."), Color::Black));
        } else if prob.existing_files.len() == 1 {
            let filename = &prob.existing_files[0];
            let f = Path::new(filename).strip_prefix(&user_directory)?;
            messages.push((
                format!(
                    "    找到文件 {} => 校验码 {}.",
                    f.display(),
                    result_into_ok_or_err(try_crc32(filename))
                ),
                Color::Black,
            ));
            if let Some(mod_date) = &prob.existing_files_date.get(filename) {
                if mod_date >= &&cfg.start_time && mod_date <= &&cfg.end_time {
                    messages.push((
                        format!("             修改日期有效 {}.", mod_date),
                        Color::Black,
                    ));
                } else {
                    messages.push((
                        format!("             修改日期不在考试时间范围内 {}.", mod_date),
                        Color::Red,
                    ));
                }
            } else {
                messages.push((format!("             文件没有修改日期记录."), Color::Yellow));
            }
        } else {
            messages.push((format!("    找到多个源代码文件:"), Color::Red));
            for file in prob.existing_files.iter() {
                messages.push((format!("        {}", file), Color::Red));
            }
        }
    }

    if let Ok(f) = if let Some(d) = std::env::args().nth(1) {
        File::open(Path::new(&d).join("checker.hash.csv"))
    } else {
        File::open("checker.hash.csv")
    } {
        messages.push((format!("{}", "正在加载比对校验文件."), Color::Yellow));
        let mut map = std::collections::HashMap::<String, Vec<(String, String)>>::new();

        let mut rdr = csv::ReaderBuilder::new().has_headers(false).from_reader(f);
        for result in rdr.records() {
            // exam_id,problem,hash,room_id,seat_id
            let record = match result {
                Ok(v) => v,
                _ => Err(CSPError::FailedToLoadCsv)?,
            };
            let student_id = &record[0];
            let problem = &record[1];
            let hash = &record[2];

            if !map.contains_key(student_id) {
                map.insert(student_id.to_string(), Vec::new());
            }
            map.get_mut(student_id)
                .unwrap()
                .push((problem.to_string(), hash.to_string()));
        }

        for prob in cfg.problems.iter() {
            let f = prob.existing_files.first();
            let real_hash = if let Some(f) = f {
                result_into_ok_or_err(try_crc32(f))
            } else {
                "文件不存在".to_string()
            };
            let submit_hash = map
                .get(student_id_found)
                .and_then(|m| {
                    m.iter()
                        .find(|(p, _)| *p == prob.name)
                        .map(|(_, h)| h.as_str())
                })
                .unwrap_or("文件不存在");

            if real_hash != *submit_hash {
                messages.push((
                    format!(
                        "题目 {} 校验值不匹配: found {}, expected {}.",
                        &prob.name, real_hash, submit_hash
                    ),
                    Color::Red,
                ));
            } else {
                messages.push((
                    format!("题目 {} 校验通过: {}.", &prob.name, submit_hash),
                    Color::Green,
                ));
            }
        }
    } else {
        messages.push((
            format!(
                "{}",
                format!("未找到校验目录的匹配项: {}", student_id_found)
            ),
            Color::Yellow,
        ));
    }

    Ok(())
}

pub fn main() {
    let mut messages = Vec::new();
    if let Err(e) = build_message(&mut messages) {
        messages.push((format!("{}", e), Color::Red));
    }
    render::render(&messages).unwrap();
}

#[derive(Debug)]
pub(crate) enum Color {
    Red,
    Yellow,
    Green,
    Black,
}
