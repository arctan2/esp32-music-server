use super::*;

pub struct DeleteMusicAsync {
    name: String
}

impl AsyncRootFn<&'static str> for DeleteMusicAsync {
    type Fut<'a> = impl core::future::Future<
        Output = Result<&'static str, FManError<<FsBlockDevice as BlockDevice>::Error>>> + 'a where Self: 'a;

    fn call<'a>(self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            let root_dir = root_dir.to_directory(vm);
            let db_dir = root_dir.open_dir(consts::DB_DIR).map_err(FManError::SdErr)?.to_raw_directory();
            let music_dir = root_dir.open_dir(consts::MUSIC_DIR).map_err(FManError::SdErr)?;

            let vm = VM::new(vm);
            let mut db = Database::new_init(vm, DbDirSdmmc::new(db_dir), ExtAlloc::default()).map_err(FManError::DbErr)?;
        
            let files_table = db.get_table(consts::MUSIC_TABLE, ExtAlloc::default()).map_err(FManError::DbErr)?;

            match music_dir.delete_file_in_dir(self.name.as_str()) {
                Err(embedded_sdmmc::Error::NotFound) => (),
                Err(e) => return Err(FManError::SdErr(e)),
                Ok(()) => ()
            }

            db.delete_from_table(files_table, Value::Chars(self.name.as_bytes()), ExtAlloc::default()).map_err(FManError::DbErr)?;

            Ok("success")
        }
    }
}

pub struct DeleteDbAsync;

impl AsyncRootFn<&'static str> for DeleteDbAsync {
    type Fut<'a> = impl core::future::Future<
        Output = Result<&'static str, FManError<<FsBlockDevice as BlockDevice>::Error>>> + 'a where Self: 'a;

    fn call<'a>(self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            let root_dir = root_dir.to_directory(vm);
            let db_dir = root_dir.open_dir(consts::DB_DIR).map_err(FManError::SdErr)?;
            match db_dir.delete_file_in_dir(alpa::WAL_FILE_NAME) {
                Err(embedded_sdmmc::Error::NotFound) => (),
                Err(e) => return Err(FManError::SdErr(e)),
                Ok(()) => ()
            }

            match db_dir.delete_file_in_dir(alpa::DB_FILE_NAME) {
                Err(embedded_sdmmc::Error::NotFound) => (),
                Err(e) => return Err(FManError::SdErr(e)),
                Ok(()) => ()
            }

            Ok("success")
        }
    }
}

pub async fn handle_delete_db() -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir_async(delete::DeleteDbAsync).await
        .map_err(|e| picoserve::response::DebugValue(e))
}

pub async fn handle_delete_music(name: String) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir_async(DeleteMusicAsync { name }).await
        .map_err(|e| picoserve::response::DebugValue(e))
}

pub async fn handle_fs_music_delete(name: String) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(move |root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);
        let files_dir = root_dir.open_dir(consts::MUSIC_DIR).map_err(FManError::SdErr)?;

        match files_dir.delete_file_in_dir(name.as_str()) {
            Err(embedded_sdmmc::Error::NotFound) => (),
            Err(e) => return Err(FManError::SdErr(e)),
            Ok(()) => ()
        }

        Ok("success")
    }).await
    .map_err(|e| picoserve::response::DebugValue(e))
}

