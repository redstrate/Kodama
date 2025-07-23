use std::cmp::Ordering;
use std::fs::read_dir;

use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Router, routing::get};
use kodama::config::get_config;
use kodama::patch::Version;
use kodama::patch::sha1::Sha1;
use kodama::{SUPPORTED_BOOT_VERSION, SUPPORTED_GAME_VERSION};
use physis::patchlist::{PatchEntry, PatchList, PatchListType};
use reqwest::header::USER_AGENT;

fn list_patch_files(dir_path: &str) -> Vec<String> {
    // If the dir doesn't exist, pretend there is no patch files
    let Ok(dir) = read_dir(dir_path) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = dir.flatten().collect();
    entries.sort_by_key(|dir| dir.path());
    let mut game_patches: Vec<_> = entries
        .into_iter()
        .flat_map(|entry| {
            let Ok(meta) = entry.metadata() else {
                return vec![];
            };
            if meta.is_dir() {
                return vec![];
            }
            if meta.is_file() && entry.file_name().to_str().unwrap().contains(".patch") {
                return vec![entry.path()];
            }
            vec![]
        })
        .collect();
    game_patches.sort_by(|a, b| {
        // Ignore H/D in front of filenames
        let a_path = a
            .as_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        if a_path.starts_with("H") {
            return Ordering::Less;
        }
        let b_path = b
            .as_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        /*if b_path.starts_with("H") {
            return Ordering::Greater;
        }*/
        a_path.partial_cmp(&b_path).unwrap()
    }); // ensure we're actually installing them in the correct order
    game_patches
        .iter()
        .map(|x| x.file_stem().unwrap().to_str().unwrap().to_string())
        .collect()
}

/// Strips the D version names
fn get_raw_version(version: &str) -> String {
    version.replace("D", "").to_string()
}

/// Check if it's a valid patch client connecting
fn check_valid_patch_client(headers: &HeaderMap) -> bool {
    let Some(user_agent) = headers.get(USER_AGENT) else {
        return false;
    };

    user_agent == "FFXIV PATCH CLIENT"
}

async fn verify_session(
    headers: HeaderMap,
    Path((channel, game_version, sid)): Path<(String, String, String)>,
    body: String,
) -> impl IntoResponse {
    if !check_valid_patch_client(&headers) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let config = get_config();

    // TODO: these are all very useful and should be documented somewhere
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Location",
        "ffxivpatch/2b5cbc63/vercheck.dat".parse().unwrap(),
    );
    headers.insert(
        "X-Repository",
        "ffxivneo/win32/release/boot".parse().unwrap(),
    );
    headers.insert("X-Patch-Module", "ZiPatch".parse().unwrap());
    headers.insert("X-Protocol", "http".parse().unwrap());
    headers.insert("X-Latest-Version", game_version.parse().unwrap());

    if config.enforce_validity_checks {
        tracing::info!("Verifying game components for {channel} {game_version} {body}...");

        let game_version = Version(&game_version);

        // Their version is too new
        if game_version > SUPPORTED_GAME_VERSION {
            tracing::warn!(
                "{game_version} is above supported game version {SUPPORTED_GAME_VERSION}!"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        // If we are up to date, yay!
        if game_version == SUPPORTED_GAME_VERSION {
            headers.insert("X-Patch-Unique-Id", sid.parse().unwrap());

            return (headers).into_response();
        }

        // check if we need any patching
        let mut send_patches = Vec::new();
        let patches = list_patch_files(&format!("{}/game", &config.patch.patches_location));

        // TODO: just don't make it take a while!
        tracing::info!(
            "Calculating SHA1 hashes for patches. This is known to take a while, sorry!"
        );

        let mut patch_length = 0;
        for patch in patches {
            let patch_str: &str = &patch;
            if game_version.partial_cmp(&Version(patch_str)).unwrap() == Ordering::Less {
                let filename = format!(
                    "{}/game/{}.patch",
                    &config.patch.patches_location, patch_str
                );
                let file = std::fs::File::open(&filename).unwrap();
                let metadata = file.metadata().unwrap();

                let sha1 = Sha1::from(std::fs::read(&filename).unwrap())
                    .digest()
                    .to_string();

                send_patches.push(PatchEntry {
                    url: format!("http://{}/game/{}.patch", config.patch.patch_dl_url, patch)
                        .to_string(),
                    version: get_raw_version(patch_str),
                    hash_block_size: metadata.len() as i64, // kind of inefficient, but whatever
                    length: metadata.len() as i64,
                    size_on_disk: metadata.len() as i64, // NOTE: wrong but it should be fine to lie
                    hashes: vec![sha1],
                    unknown_a: 19,
                    unknown_b: 18,
                });
                patch_length += metadata.len();
            }
        }

        if !send_patches.is_empty() {
            headers.insert("X-Patch-Unique-Id", sid.parse().unwrap());
            headers.insert(
                "Content-Type",
                "multipart/mixed; boundary=477D80B1_38BC_41d4_8B48_5273ADB89CAC"
                    .parse()
                    .unwrap(),
            );

            let patch_list = PatchList {
                id: "477D80B1_38BC_41d4_8B48_5273ADB89CAC".to_string(),
                requested_version: game_version.to_string().clone(),
                content_location: format!("ffxivpatch/2b5cbc63/metainfo/{}.http", game_version.0), // FIXME: i think this is actually supposed to be the target version
                patch_length,
                patches: send_patches,
            };
            let patch_list_str = patch_list.to_string(PatchListType::Game);
            return (headers, patch_list_str).into_response();
        }
    }

    (headers).into_response()
}

async fn verify_boot(
    headers: HeaderMap,
    Path((channel, boot_version)): Path<(String, String)>,
) -> impl IntoResponse {
    if !check_valid_patch_client(&headers) {
        tracing::warn!("Invalid patch client! {headers:#?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let config = get_config();

    // TODO: these are all very useful and should be documented somewhere
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Location",
        "ffxivpatch/2b5cbc63/vercheck.dat".parse().unwrap(),
    );
    headers.insert(
        "X-Repository",
        "ffxivneo/win32/release/boot".parse().unwrap(),
    );
    headers.insert("X-Patch-Module", "ZiPatch".parse().unwrap());
    headers.insert("X-Protocol", "http".parse().unwrap());
    headers.insert("X-Latest-Version", boot_version.parse().unwrap());

    if config.enforce_validity_checks {
        tracing::info!("Verifying boot components for {channel} {boot_version}...");

        let boot_version = Version(&boot_version);

        // If we are up to date, yay!
        if boot_version == SUPPORTED_BOOT_VERSION {
            return (headers).into_response();
        }

        // Their version is too new
        if boot_version > SUPPORTED_BOOT_VERSION {
            tracing::warn!(
                "{boot_version} is above supported boot version {SUPPORTED_BOOT_VERSION}!"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        // check if we need any patching
        let mut send_patches = Vec::new();
        let patches = list_patch_files(&format!("{}/boot", &config.patch.patches_location));
        let mut patch_length = 0;
        for patch in patches {
            let patch_str: &str = &patch;
            if boot_version.partial_cmp(&Version(patch_str)).unwrap() == Ordering::Less {
                let file = std::fs::File::open(&*format!(
                    "{}/boot/{}.patch",
                    &config.patch.patches_location, patch_str
                ))
                .unwrap();
                let metadata = file.metadata().unwrap();

                send_patches.push(PatchEntry {
                    url: format!("http://{}/boot/{}.patch", config.patch.patch_dl_url, patch)
                        .to_string(),
                    version: get_raw_version(patch_str),
                    hash_block_size: 0,
                    length: metadata.len() as i64,
                    size_on_disk: metadata.len() as i64, // NOTE: wrong but it should be fine to lie
                    hashes: vec![],
                    unknown_a: 19,
                    unknown_b: 18,
                });
                patch_length += metadata.len();
            }
        }

        if !send_patches.is_empty() {
            headers.insert(
                "Content-Type",
                "multipart/mixed; boundary=477D80B1_38BC_41d4_8B48_5273ADB89CAC"
                    .parse()
                    .unwrap(),
            );

            let patch_list = PatchList {
                id: "477D80B1_38BC_41d4_8B48_5273ADB89CAC".to_string(),
                requested_version: boot_version.to_string().clone(),
                content_location: format!("ffxivpatch/2b5cbc63/metainfo/{}.http", boot_version.0), // FIXME: i think this is actually supposed to be the target version
                patch_length,
                patches: send_patches,
            };
            let patch_list_str = patch_list.to_string(PatchListType::Boot);
            return (headers, patch_list_str).into_response();
        }
    }

    (headers).into_response()
}

async fn fallback(uri: Uri) -> (StatusCode, String) {
    tracing::warn!("{}", uri);
    (StatusCode::NOT_FOUND, format!("No route for {uri}"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/http/{channel}/{game_version}/{sid}", post(verify_session))
        .route("/http/{channel}/{boot_version}", get(verify_boot))
        .fallback(fallback);

    let config = get_config();

    let addr = config.patch.get_socketaddr();
    tracing::info!("Server started on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
