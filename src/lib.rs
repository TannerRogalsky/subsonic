pub mod api {
    pub type SubsonicResponse = GenericSubsonicResponse<Response>;

    #[derive(Debug, serde::Deserialize)]
    pub struct GenericSubsonicResponse<T> {
        #[serde(rename = "subsonic-response")]
        pub subsonic_response: Container<T>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct Container<T> {
        pub version: String,
        pub status: String,
        #[serde(flatten)]
        pub content: T,
    }

    include!(concat!(env!("OUT_DIR"), "/api.rs"));
}

pub struct Client {
    base_url: reqwest::Url,
    auth: Auth,
    inner: reqwest::Client,
    version: semver::Version,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .field("auth", &self.auth)
            .field("version", &self.version)
            .finish()
    }
}

impl Client {
    pub fn new<U: reqwest::IntoUrl>(
        base_url: U,
        user: String,
        password: String,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let base_url = base_url.into_url()?;
        let auth = Auth { user, password };
        // TODO: pull from API
        let version = semver::Version::parse("1.16.1")?;
        Ok(Self {
            base_url,
            auth,
            inner: reqwest::Client::new(),
            version,
        })
    }

    pub async fn ping(&self) -> Result<bool> {
        let response = self
            .get("ping")
            .send()
            .await?
            .json::<api::GenericSubsonicResponse<()>>()
            .await?;
        Ok(response.subsonic_response.status == "ok")
    }

    fn get(&self, query: &str) -> reqwest::RequestBuilder {
        let mut url = self
            .base_url
            .join("rest/")
            .and_then(|url| url.join(query))
            .unwrap();
        url.query_pairs_mut()
            .extend_pairs(self.auth.to_query(&self.version));
        self.inner.get(url)
    }

    fn get_with_params<P: serde::Serialize>(
        &self,
        query: &str,
        params: &P,
    ) -> reqwest::RequestBuilder {
        let mut url = self
            .base_url
            .join("rest/")
            .and_then(|url| url.join(query))
            .unwrap();
        url.query_pairs_mut()
            .extend_pairs(self.auth.to_query(&self.version))
            .append_key_only(&serde_qs::to_string(params).unwrap());
        self.inner.get(url)
    }
}

struct Auth {
    user: String,
    password: String,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("user", &self.user)
            .field("password", &"<secret>")
            .finish()
    }
}

impl Auth {
    const SALT_SIZE: usize = 36; // Minimum 6 characters.

    fn to_query(&self, version: &semver::Version) -> impl Iterator<Item = (&'static str, String)> {
        let mut pairs = Vec::with_capacity(6);
        let good_auth_version = semver::Comparator {
            op: semver::Op::GreaterEq,
            major: 1,
            minor: Some(13),
            patch: None,
            pre: Default::default(),
        };
        if good_auth_version.matches(&version) {
            use rand::{distributions::Alphanumeric, Rng};

            let salt: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(Self::SALT_SIZE)
                .map(char::from)
                .collect();
            let pre_t = self.password.to_string() + &salt;
            let token = format!("{:x}", md5::compute(pre_t.as_bytes()));

            pairs.push(("u", self.user.clone()));
            pairs.push(("t", token));
            pairs.push(("s", salt));
        } else {
            pairs.push(("u", self.user.clone()));
            pairs.push(("p", self.password.clone()));
        };

        let format = "json";
        let crate_name = env!("CARGO_PKG_NAME");

        pairs.push(("v", version.to_string()));
        pairs.push(("c", crate_name.to_string()));
        pairs.push(("f", format.to_string()));

        pairs.into_iter()
    }
}

pub type Result<T> = reqwest::Result<T>;

#[derive(Debug)]
pub enum SubsonicResponseError {
    ApiError(api::Error),
    TypeError(api::Response),
}

impl From<api::Response> for SubsonicResponseError {
    fn from(response: api::Response) -> Self {
        match response {
            api::Response::Error(error) => Self::ApiError(error),
            _ => Self::TypeError(response),
        }
    }
}

impl std::fmt::Display for SubsonicResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for SubsonicResponseError {}

#[derive(Debug)]
pub struct SubsonicResponse<T> {
    pub version: String,
    pub result: std::result::Result<T, SubsonicResponseError>,
}

impl api::License {
    pub async fn get(client: &Client) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getLicense")
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::MusicFolders {
    pub async fn get(client: &Client) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getMusicFolders")
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct IndexesConfig<'a> {
    #[serde(rename = "musicFolderId")]
    pub music_folder_id: Option<std::borrow::Cow<'a, str>>,
    #[serde(rename = "ifModifiedSince")]
    pub if_modified_since: Option<u64>,
}

impl api::Indexes {
    pub async fn get(client: &Client) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getIndexes")
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }

    pub async fn get_with_config(
        client: &Client,
        config: &IndexesConfig<'_>,
    ) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get_with_params("getIndexes", &config)
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::Directory {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getMusicDirectory")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::Genres {
    pub async fn get(client: &Client) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getGenres")
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::ArtistsID3 {
    pub async fn get<ID: AsRef<str>>(
        client: &Client,
        music_folder_id: Option<ID>,
    ) -> Result<SubsonicResponse<Self>> {
        let req = client.get("getArtists");
        let req = if let Some(music_folder_id) = music_folder_id {
            req.query(&[("musicFolderId", music_folder_id.as_ref())])
        } else {
            req
        };
        Ok(req
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::ArtistWithAlbumsID3 {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getArtist")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::AlbumWithSongsID3 {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getAlbum")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::Child {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getSong")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::Videos {
    pub async fn get(client: &Client) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getVideos")
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::VideoInfo {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getVideoInfo")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::ArtistInfo {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getArtistInfo")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::ArtistInfo2 {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getArtistInfo2")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::AlbumInfo {
    pub async fn get<ID: AsRef<str>>(client: &Client, id: ID) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getAlbumInfo")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }

    pub async fn get_id3<ID: AsRef<str>>(
        client: &Client,
        id: ID,
    ) -> Result<SubsonicResponse<Self>> {
        Ok(client
            .get("getAlbumInfo2")
            .query(&[("id", id.as_ref())])
            .send()
            .await?
            .json::<api::SubsonicResponse>()
            .await?
            .into())
    }
}

impl api::Child {
    pub async fn download(&self, client: &Client) -> Result<reqwest::Response> {
        let request = self.download_request(client)?;
        client.inner.execute(request).await
    }

    pub fn download_request(&self, client: &Client) -> Result<reqwest::Request> {
        client
            .get("download")
            .query(&[("id", self.id.as_str())])
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct Config {
        user: String,
        password: String,
        url: String,
    }

    #[tokio::test]
    async fn ping() {
        dotenv::dotenv().unwrap();
        let config: Config = envy::prefixed("SUBSONIC_").from_env().unwrap();

        let client = Client::new(config.url, config.user, config.password).unwrap();
        let success = client.ping().await.unwrap();
        assert!(success);
    }

    #[tokio::test]
    async fn get_artists() {
        dotenv::dotenv().unwrap();
        let config: Config = envy::prefixed("SUBSONIC_").from_env().unwrap();
        let client = Client::new(config.url, config.user, config.password).unwrap();

        let response: SubsonicResponse<api::ArtistsID3> = client
            .get("getArtists")
            .send()
            .await
            .unwrap()
            .json::<api::SubsonicResponse>()
            .await
            .unwrap()
            .into();
        assert!(response.result.is_ok())
    }

    #[tokio::test]
    async fn get_album() {
        dotenv::dotenv().unwrap();
        let config: Config = envy::prefixed("SUBSONIC_").from_env().unwrap();

        let client = Client::new(config.url, config.user, config.password).unwrap();
        let indexes = api::Indexes::get(&client).await.unwrap().result.unwrap();
        let artist = api::ArtistWithAlbumsID3::get(&client, &indexes.index[0].artist[0].id)
            .await
            .unwrap()
            .result
            .unwrap();
        let album = api::AlbumWithSongsID3::get(&client, &artist.album[0].id)
            .await
            .unwrap()
            .result
            .unwrap();
        assert!(!album.song.is_empty());
        let album_info = api::AlbumInfo::get(&client, &artist.album[0].id)
            .await
            .unwrap()
            .result;
        assert!(album_info.is_ok());
    }
}
