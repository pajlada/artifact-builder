use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Root {
    #[serde(rename = "ref")]
    pub push_ref: String,

    // The commit hash before this push
    pub before: String,

    // The commit hash after this push
    pub after: String,

    pub repository: Repository,

    pub sender: Sender,

    pub head_commit: HeadCommit,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,

    pub name: String,
    pub full_name: String,
    pub owner: Owner,
    pub description: Option<String>,
    pub fork: bool,
    pub created_at: i64,
    pub updated_at: String,
    pub pushed_at: i64,
    pub default_branch: String,
    pub master_branch: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Owner {
    pub id: i64,

    pub name: String,
    pub email: String,
    pub login: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sender {
    pub id: i64,

    pub login: String,
    pub url: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,

    pub tree_id: String,
    pub distinct: bool,
    pub message: String,
    pub timestamp: String,

    // pub author: Author,
    pub committer: Committer,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub username: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Committer {
    pub name: String,
    pub email: String,
    pub username: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadCommit {
    pub id: String,
    pub tree_id: String,
    pub distinct: bool,
    pub message: String,
    pub timestamp: String,
    pub url: String,
    pub author: Author,
    pub committer: Committer,
}
