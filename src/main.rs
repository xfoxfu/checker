mod config;
mod render;

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

fn build_message() -> Vec<(String, Color)> {
    let mut message = Vec::new();
    let cfg_file = if let Some(d) = std::env::args().nth(1) {
        File::open(Path::new(&d).join("checker.cfg.json"))
    } else {
        File::open("checker.cfg.json")
    };
    if cfg_file.is_err() {
        return vec![(
            format!("错误 1, checker.cfg.json 不存在, 请联系监考员."),
            Color::Red,
        )];
    }
    let cfg = serde_json::from_reader(cfg_file.unwrap());
    if cfg.is_err() {
        return vec![(
            format!("错误 2, checker.cfg.json 无法解析, 请联系监考员."),
            Color::Red,
        )];
    }
    let mut cfg: Contestant = cfg.unwrap();

    let mut valid_folders = Vec::new();

    for dir in read_dir(&cfg.root_path).unwrap() {
        let dir = dir.unwrap();
        if dir.file_type().unwrap().is_dir()
            && cfg.regex.is_match(dir.file_name().to_str().unwrap())
        {
            valid_folders.push(dir.path());
        }
    }

    if valid_folders.is_empty() {
        return vec![(
            format!("错误 3, 没有找到有效的选手目录. 请阅读考生须知."),
            Color::Red,
        )];
    }

    if valid_folders.len() > 1 {
        message.push((format!("找到多个选手目录: "), Color::Red));
        for valid_folder in valid_folders.iter() {
            message.push((format!("    {:?}", valid_folder), Color::Red));
        }
        return vec![(format!("错误 4, 找到多个选手目录."), Color::Red)];
    }

    let valid_folder_name = valid_folders.into_iter().next().unwrap();
    let user_directory = valid_folder_name;
    let student_id_found = Path::new(&user_directory)
        .strip_prefix(&cfg.root_path)
        .unwrap()
        .to_str()
        .unwrap();

    message.push((
        format!(
            "找到选手目录： {}, 请确认是否与准考证号一致.",
            student_id_found
        ),
        Color::Yellow,
    ));

    for dir1 in read_dir(&user_directory).unwrap() {
        let dir1 = dir1.unwrap();
        if !dir1.file_type().unwrap().is_dir() {
            continue;
        }
        for dir2 in read_dir(dir1.path()).unwrap() {
            let dir2 = dir2.unwrap();
            if !dir2.file_type().unwrap().is_file() {
                continue;
            }
            for prob in cfg.problems.iter_mut() {
                if prob.regex.is_match(
                    dir2.path()
                        .strip_prefix(&user_directory)
                        .unwrap()
                        .to_str()
                        .unwrap(),
                ) {
                    prob.existing_files
                        .push(dir2.path().to_str().unwrap().to_string());
                }
            }
        }
    }

    for prob in cfg.problems.iter() {
        print!("题目 {}: ", prob.name);
        if prob.existing_files.is_empty() {
            message.push((format!("未找到源代码文件."), Color::Black));
        } else if prob.existing_files.len() == 1 {
            let f = Path::new(&prob.existing_files[0])
                .strip_prefix(&user_directory)
                .unwrap();
            message.push((
                format!(
                    "找到文件 {} => 校验码 {}.",
                    f.display(),
                    result_into_ok_or_err(try_crc32(&prob.existing_files[0]))
                ),
                Color::Black,
            ));
        } else {
            message.push((format!("找到多个源代码文件:"), Color::Red));
            for file in prob.existing_files.iter() {
                message.push((format!("    {}", file), Color::Red));
            }
        }
    }

    if let Ok(f) = if let Some(d) = std::env::args().nth(1) {
        File::open(Path::new(&d).join("checker.hash.csv"))
    } else {
        File::open("checker.hash.csv")
    } {
        message.push((format!("{}", "正在加载比对校验文件."), Color::Yellow));
        let mut map = std::collections::HashMap::<String, Vec<(String, String)>>::new();

        let mut rdr = csv::ReaderBuilder::new().has_headers(false).from_reader(f);
        for result in rdr.records() {
            // exam_id,problem,hash,room_id,seat_id
            let record = match result {
                Ok(v) => v,
                _ => return vec![(format!("无法解析 CSV 文件"), Color::Red)],
            };
            let student_id = &record[0];
            let problem = &record[1];
            let hash = &record[2];
            let _room_id = &record[3];
            let _seat_id = &record[4];

            if !map.contains_key(student_id) {
                map.insert(student_id.to_string(), Vec::new());
            }
            map.get_mut(student_id)
                .unwrap()
                .push((problem.to_string(), hash.to_string()));
        }

        if let Some(s) = map.get(student_id_found) {
            for (p, h) in s.iter() {
                let file = &cfg
                    .problems
                    .iter()
                    .find(|prob| prob.name == *p)
                    .unwrap()
                    .existing_files
                    .first();
                let real_hash = if let Some(f) = file {
                    result_into_ok_or_err(try_crc32(f))
                } else {
                    "文件不存在".to_string()
                };
                if real_hash != *h {
                    message.push((
                        format!(
                            "{}",
                            format!(
                                "题目 {} 校验值不匹配: found {}, expected {}.",
                                p, real_hash, h
                            )
                        ),
                        Color::Red,
                    ));
                } else {
                    message.push((
                        format!("{}", format!("题目 {} 校验通过: {}.", p, h)),
                        Color::Green,
                    ));
                }
            }
        } else {
            message.push((
                format!(
                    "{}",
                    format!("未找到校验目录的匹配项: {}", student_id_found)
                ),
                Color::Yellow,
            ));
        }
    }

    message
}

pub fn main() {
    let message = build_message();
    render::render(&message).unwrap();
}

#[derive(Debug)]
pub(crate) enum Color {
    Red,
    Yellow,
    Green,
    Black,
}
