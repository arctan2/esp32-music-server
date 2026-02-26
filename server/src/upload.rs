use super::*;
use picoserve::response::{IntoResponse};
use file_manager::{IntAlloc};
use alpa::{db::Database, Row};
use embedded_sdmmc::Mode;
use picoserve::response::{DebugValue};

#[cfg(feature = "std-mode")]
use std::println;

#[cfg(feature = "embassy-mode")]
use esp_println::println;

#[derive(serde::Deserialize)]
pub struct NewQuery {
    name: String,
    ext: String,
    size: usize
}

#[derive(serde::Deserialize)]
pub struct EndQuery {
    id: String,
}

pub async fn new(dir_name: String, query: picoserve::extract::Query<NewQuery>) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(move |root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);

        if dir_name != consts::MUSIC_DIR && dir_name != consts::FILES_DIR {
            return Err("invalid dir name".into());
        }

        let db_dir = root_dir.open_dir(consts::DB_DIR).map_err(|_| "unable to open db dir")?.to_raw_directory();
        let db_dir = DbDirSdmmc::new(db_dir);
        let mut db = match Database::new_init(VM::new(vm), db_dir, IntAlloc::default()) {
            Ok(d) => d,
            Err(_) => return Err("db init error".into())
        };
        let count_tracker_table = db.get_table(consts::COUNT_TRACKER_TABLE, IntAlloc::default())
                                    .map_err(|_| "unable to get count_tracker table")?;
        let music_table = db.get_table(dir_name.as_str(), IntAlloc::default())
                            .map_err(|_| "unable to get music table")?;
        let cur_file_id: i64;

        {
            let query = Query::<_, &str>::new(count_tracker_table, IntAlloc::default())
                                         .key(Value::Chars(dir_name.as_bytes()));
            match QueryExecutor::new(
                query, &mut db.table_buf, &mut db.buf1, &mut db.buf2,
                &db.file_handler.page_rw.as_ref().unwrap()
            ) {
                Ok(mut exec) => {
                    if let Ok(row) = exec.next() {
                        cur_file_id = row[1].to_int().unwrap();
                    } else {
                        return Err("bad init".into());
                    }
                },
                Err(_) => {
                    return Err("table empty".into());
                }
            };
        }

        if cur_file_id < 0 || cur_file_id >= 99999999 {
            return Err("id limit reached".into());
        }

        let music_dir = root_dir.open_dir(dir_name.as_str()).map_err(|_| "unable to open dir")?;
        let actual_name = format!("{}.{}", cur_file_id, query.ext);
        let file = music_dir.open_file_in_dir(actual_name.as_str(), Mode::ReadWriteCreate)
                            .map_err(|e| {
                                println!("e = {:?}", e);
                                "unable to create new file"
                            })?;

        file.close().map_err(|_| "unable to close new file")?;

        {
            let mut row = Row::new_in(IntAlloc::default());
            row.push(Value::Chars(actual_name.as_bytes()));
            row.push(Value::Chars(&query.name.as_bytes()));
            row.push(Value::Int(query.size as i64));
            db.insert_to_table(music_table, row, IntAlloc::default()).map_err(|e| {
                println!("e = {:?}", e);
                "unable to insert to table"
            })?;
        }

        {
            let mut row = Row::new_in(IntAlloc::default());
            row.push(Value::Chars(dir_name.as_bytes()));
            row.push(Value::Int(cur_file_id + 1));
            db.update_row(count_tracker_table, Value::Chars(dir_name.as_bytes()), row, IntAlloc::default())
                .map_err(|_| "unable to update count_tracker_table to table")?;
        }

        Ok(DebugValue(format!("success: {}", actual_name)))
    }).await
    .map_err(|e| {
        DebugValue(e)
    })
}

pub struct MusicReceiver {
    response: String
}

impl<'r, State> FromRequest<'r, State> for MusicReceiver {
    type Rejection = picoserve::response::DebugValue<String>;

    async fn from_request<R: Read>(
        _state: &'r State,
        parts: RequestParts<'r>,
        body: RequestBody<'r, R>,
    ) -> Result<Self, Self::Rejection> {
        let last_segment = parts.path().segments().last().ok_or(DebugValue(String::from("path not mentioned")))?.0;
        if last_segment != consts::MUSIC_DIR && last_segment != consts::FILES_DIR {
            return Err(DebugValue(String::from("invalid dir name")));
        }

        chunk::receive_chunks(parts, body, last_segment).await.map(|response| Self { response })
    }
}

pub async fn chunk(_: String, data: MusicReceiver) -> impl IntoResponse {
    picoserve::response::DebugValue(data.response)
}

pub async fn end(dir_name: String, query: picoserve::extract::Query<EndQuery>) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(move |root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);

        if dir_name != consts::MUSIC_DIR && dir_name != consts::FILES_DIR {
            return Err("invalid dir name".into());
        }

        let music_dir = root_dir.open_dir(dir_name.as_str()).map_err(|_| "unable to open dir")?;
        let _ = music_dir.open_file_in_dir(query.id.as_str(), Mode::ReadWriteAppend).map_err(|_| "file not found")?;
        Ok("success")
    }).await
    .map_err(|e| picoserve::response::DebugValue(e))
}
