pub mod tooling;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ToonCapability {
    pub module: &'static str,
    pub capabilities: [&'static str; 4],
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub owner_user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct EpisodeSummary {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub episode_no: i32,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateEpisodeRequest {
    pub title: String,
    pub episode_no: i32,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEpisodeRequest {
    pub title: String,
    pub episode_no: i32,
    pub summary: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SceneSummary {
    pub id: String,
    pub episode_id: String,
    pub title: String,
    pub scene_no: i32,
    pub content: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSceneRequest {
    pub title: String,
    pub scene_no: i32,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSceneRequest {
    pub title: String,
    pub scene_no: i32,
    pub content: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct PublishProjectRequest {
    pub channel: String,
}

impl Default for ToonCapability {
    fn default() -> Self {
        Self {
            module: "toon",
            capabilities: ["toon-project", "episode", "scene", "publishing"],
        }
    }
}
