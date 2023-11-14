use std::fs::{DirEntry, FileType, Metadata};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Problem {
    /// 题目名称，如 `math`
    pub name: String,
    #[serde(with = "regex_sd")]
    pub regex: Regex,
    #[serde(skip)]
    pub existing_files: Vec<FileEntry>,
}

#[derive(Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub file_type: FileType,
    pub metadata: Metadata,
}

impl FileEntry {
    pub fn from(entry: &DirEntry) -> Result<Self, std::io::Error> {
        Ok(Self {
            path: entry.path(),
            file_type: entry.file_type()?,
            metadata: entry.metadata()?,
        })
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Contestant {
    /// 选手文件夹父路径
    pub root_path: String,
    #[serde(with = "regex_sd")]
    pub regex: Regex,
    /// 所有题目的配置项
    pub problems: Vec<Problem>,
    /// 考试开始时间
    pub start_time: DateTime<Utc>,
    /// 考试结束时间
    pub end_time: DateTime<Utc>,
    /// 文件大小限制
    pub size_limit_kb: u64,
}

mod regex_sd {
    use regex::Regex;
    use serde::Deserialize;
    use std::str::FromStr;

    pub fn serialize<S>(r: &Regex, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&r.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Regex, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        #[cfg(unix)]
        let s = s.replace(r"\\", "/");
        Regex::from_str(&s).map_err(serde::de::Error::custom)
    }
}
