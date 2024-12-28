use std::{cmp::Ordering, collections::HashMap};

use image::GenericImageView;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use serde::Serialize;
use serde_json::Value;

const SPOTIFY_URL: &str = "https://open.spotify.com/";

pub struct Spotify {
    client: Client,
}

impl Spotify {
    pub fn new(bearer_token: String, client_token: Option<String>) -> Self {
        let mut headers = HeaderMap::new();
        headers.append("App-Platform", HeaderValue::from_static("WebPlayer"));
        headers.append("User-Agent", HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"));
        headers.append(
            "Authorization",
            HeaderValue::from_str(&bearer_token).unwrap(),
        );
        if let Some(client_token) = client_token {
            headers.append(
                "Client-Token",
                HeaderValue::from_str(&client_token).unwrap(),
            );
        }
        Spotify {
            client: Client::builder().default_headers(headers).build().unwrap(),
        }
    }
    pub async fn sort(&mut self, playlist_code: String, offset: usize, limit: usize) {
        let query = Query::new(Variables::FetchPlaylistWithGatedEntityRelations {
            playlist_uri: format!("spotify:playlist:{playlist_code}"),
            offset,
            limit,
        });
        let request = self
            .client
            .post("https://api-partner.spotify.com/pathfinder/v1/query")
            .json(&query)
            .build()
            .unwrap();
        let response = self
            .client
            .execute(request)
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let response: HashMap<String, Value> = serde_json::from_str(&response).unwrap();
        let response = response.get("data").unwrap().get("playlistV2").unwrap();
        let content = response.get("content").unwrap().get("items").unwrap();
        let mut tracks = Vec::new();
        for item in content.as_array().unwrap() {
            let uid = item.get("uid").unwrap().as_str().unwrap();
            let item = item.get("itemV2").unwrap().get("data").unwrap();
            let cover_art_url = item
                .get("albumOfTrack")
                .unwrap()
                .get("coverArt")
                .unwrap()
                .get("sources")
                .unwrap()
                .as_array()
                .unwrap()
                .get(1)
                .unwrap()
                .get("url")
                .unwrap()
                .as_str()
                .unwrap();
            let name = item.get("name").unwrap().as_str().unwrap();
            let request = self.client.get(cover_art_url).build().unwrap();
            let response = self.client.execute(request).await.unwrap();
            let image = image::load_from_memory(&response.bytes().await.unwrap()).unwrap();
            let mut r_sum: usize = 0;
            let mut g_sum: usize = 0;
            let mut b_sum: usize = 0;
            for (_, _, color) in image.pixels() {
                // TODO: improve algorithm!
                let colors = color.0;
                r_sum += colors[0] as usize;
                g_sum += colors[1] as usize;
                b_sum += colors[2] as usize;
            }
            let (w, h) = image.dimensions();
            let total = w as usize * h as usize;
            let colors = lab::Lab::from_rgb(&[
                (r_sum / total).try_into().unwrap(),
                (g_sum / total).try_into().unwrap(),
                (b_sum / total).try_into().unwrap(),
            ]);
            tracks.push(Track {
                uid: uid.into(),
                comparison: Comparison { lab: colors },
                name: name.into(),
            });
        }
        println!("Found {} Tracks", tracks.len());
        tracks.sort_by(|a, b| {
            const REFERENCE_COLOR: lab::Lab = lab::Lab {
                l: 10000.0,
                a: 0.0,
                b: 0.0,
            };
            let a = a.comparison.lab.squared_distance(&REFERENCE_COLOR);
            let b = b.comparison.lab.squared_distance(&REFERENCE_COLOR);
            a.partial_cmp(&b).unwrap()
        });
        if let Some(mut previous_track) = tracks.first() {
            for track in tracks.iter().skip(1) {
                let query = Query::new(Variables::MoveItemsInPlaylist {
                    playlist_uri: format!("spotify:playlist:{playlist_code}"),
                    new_position: Position {
                        from_uid: previous_track.uid.clone(),
                        move_type: MoveType::AfterUID,
                    },
                    uids: vec![track.uid.clone()],
                });
                let request = self
                    .client
                    .post("https://api-partner.spotify.com/pathfinder/v1/query")
                    .json(&query)
                    .build()
                    .unwrap();
                let response = self.client.execute(request).await.unwrap();
                println!("Sorted {track:?}");
                println!("Response: {}", response.text().await.unwrap());
                previous_track = track;
            }
        }
        println!("Tracks sorted: {}", tracks.len());
    }
}

#[derive(Debug)]
struct Track {
    uid: String,
    comparison: Comparison,
    name: String,
}

#[derive(Debug)]
struct Comparison {
    lab: lab::Lab,
}

#[derive(Serialize)]
struct Query {
    #[serde(rename = "operationName")]
    operation: Operation,
    variables: Variables,
    extensions: Extensions,
}

impl Query {
    pub fn new(variables: Variables) -> Self {
        Query {
            operation: variables.operation(),
            extensions: Extensions {
                persisted_query: PersistedQuery::new(variables.query_type()),
            },
            variables,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Operation {
    FetchPlaylistWithGatedEntityRelations,
    MoveItemsInPlaylist,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Variables {
    FetchPlaylistWithGatedEntityRelations {
        #[serde(rename = "uri")]
        playlist_uri: String,
        offset: usize,
        limit: usize,
    },
    #[serde(rename_all = "camelCase")]
    MoveItemsInPlaylist {
        playlist_uri: String,
        new_position: Position,
        uids: Vec<String>,
    },
}

impl Variables {
    pub fn operation(&self) -> Operation {
        match self {
            Variables::FetchPlaylistWithGatedEntityRelations { .. } => {
                Operation::FetchPlaylistWithGatedEntityRelations
            }
            Variables::MoveItemsInPlaylist { .. } => Operation::MoveItemsInPlaylist,
        }
    }
    pub fn query_type(&self) -> QueryType {
        match self {
            Variables::FetchPlaylistWithGatedEntityRelations { .. } => {
                QueryType::FetchPlaylistWithGatedEntityRelations
            }
            Variables::MoveItemsInPlaylist { .. } => QueryType::MoveItemsInPlaylist,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Position {
    from_uid: String,
    move_type: MoveType,
}

#[derive(Serialize)]
enum MoveType {
    #[serde(rename = "AFTER_UID")]
    AfterUID,
    #[serde(rename = "BEFORE_UID")]
    BeforeUID,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Extensions {
    persisted_query: PersistedQuery,
}

#[derive(Serialize)]
struct PersistedQuery {
    #[serde(rename = "sha256Hash")]
    query_type: QueryType,
    version: usize,
}

impl PersistedQuery {
    pub fn new(query_type: QueryType) -> Self {
        PersistedQuery {
            query_type,
            version: 1,
        }
    }
}

#[derive(Serialize)]
enum QueryType {
    #[serde(rename = "19ff1327c29e99c208c86d7a9d8f1929cfdf3d3202a0ff4253c821f1901aa94d")]
    FetchPlaylistWithGatedEntityRelations,
    #[serde(rename = "47c69e71df79e3c80e4af7e7a9a727d82565bb20ae20dc820d6bc6f94def482d")]
    MoveItemsInPlaylist,
}
