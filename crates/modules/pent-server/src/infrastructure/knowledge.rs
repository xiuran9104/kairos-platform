//! Knowledge base backed by pgvector.
//!
//! Supports three knowledge types associated with Flow/Task/Subtask:
//!   - guide: methodology, policy, attack guide
//!   - answer: Q&A pairs generated during pentesting
//!   - code: code snippets, exploits, scripts
//!
//! Flow/Task/Subtask IDs link knowledge to specific execution contexts.

use kairos_platform_framework_database::PgPool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeDocType {
    Guide,
    Answer,
    Code,
}

impl KnowledgeDocType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Guide => "guide",
            Self::Answer => "answer",
            Self::Code => "code",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "guide" => Self::Guide,
            "answer" => Self::Answer,
            "code" => Self::Code,
            _ => Self::Guide,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FlowContext {
    None = 0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: Uuid,
    pub doc_type: String,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub tags: Value,
    pub flow_id: Option<i32>,
    pub task_id: Option<i32>,
    pub subtask_id: Option<i32>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub answer_type: Option<String>,
    pub guide_type: Option<String>,
    pub code_lang: Option<String>,
    pub manual: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeSearchResult {
    pub document: KnowledgeDocument,
    pub score: f32,
}

// ── Insert params ─────────────────────────────────────────────────────────

pub struct InsertKnowledgeParams {
    pub doc_type: KnowledgeDocType,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub tags: Value,
    pub flow_id: Option<i32>,
    pub task_id: Option<i32>,
    pub subtask_id: Option<i32>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub guide_type: Option<String>,
    pub answer_type: Option<String>,
    pub code_lang: Option<String>,
    pub manual: bool,
}

pub struct KnowledgeStore {
    pool: PgPool,
}

// ── CRUD ──────────────────────────────────────────────────────────────────

impl KnowledgeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, params: InsertKnowledgeParams) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO pent_knowledge_documents(
                id, doc_type, title, content, source, tags,
                flow_id, task_id, subtask_id,
                question, description, guide_type, answer_type, code_lang, manual
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(id)
        .bind(params.doc_type.as_str())
        .bind(&params.title)
        .bind(&params.content)
        .bind(&params.source)
        .bind(&params.tags)
        .bind(params.flow_id)
        .bind(params.task_id)
        .bind(params.subtask_id)
        .bind(&params.question)
        .bind(&params.description)
        .bind(&params.guide_type)
        .bind(&params.answer_type)
        .bind(&params.code_lang)
        .bind(params.manual)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// Store embedding vector for a document.
    pub async fn upsert_embedding(
        &self,
        document_id: Uuid,
        embedding: &[f32],
        metadata: &Value,
    ) -> Result<Uuid, String> {
        let emb_str = Self::format_vector(embedding);
        let embedding_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO langchain_pg_embedding(id, document, embedding, cmetadata)
             VALUES($1, '', $2::vector, $3)",
        )
        .bind(embedding_id)
        .bind(&emb_str)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("UPDATE pent_knowledge_documents SET embedding_id = $2 WHERE id = $1")
            .bind(document_id)
            .bind(embedding_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(embedding_id)
    }

    /// Vector similarity search with optional type and flow filters.
    pub async fn search(
        &self,
        query_embedding: &[f32],
        filter: &KnowledgeFilter,
        top_k: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, String> {
        let emb_str = Self::format_vector(query_embedding);
        let rows = sqlx::query_as::<_, SearchRow>(
            "SELECT d.id, d.doc_type, d.title, d.content, d.source, d.tags,
                    d.flow_id, d.task_id, d.subtask_id,
                    d.question, d.description, d.guide_type, d.answer_type, d.code_lang,
                    d.manual, d.created_at,
                    1.0 - (e.embedding <=> $1::vector) AS similarity
             FROM pent_knowledge_documents d
             JOIN langchain_pg_embedding e ON d.embedding_id = e.id
             WHERE ($2::text IS NULL OR d.doc_type = $2)
               AND ($3::int IS NULL OR d.flow_id = $3)
               AND ($4::int IS NULL OR d.task_id = $4)
               AND ($5::int IS NULL OR d.subtask_id = $5)
               AND ($6::text IS NULL OR d.guide_type = $6)
               AND ($7::text IS NULL OR d.answer_type = $7)
               AND ($8::text IS NULL OR d.code_lang = $8)
             ORDER BY similarity DESC
             LIMIT $9",
        )
        .bind(&emb_str)
        .bind(filter.doc_type.as_deref())
        .bind(filter.flow_id)
        .bind(filter.task_id)
        .bind(filter.subtask_id)
        .bind(filter.guide_type.as_deref())
        .bind(filter.answer_type.as_deref())
        .bind(filter.code_lang.as_deref())
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(|r| r.into_result()).collect())
    }

    /// Full-text search fallback (no embedding needed).
    pub async fn search_text(
        &self,
        query: &str,
        filter: &KnowledgeFilter,
        limit: usize,
    ) -> Result<Vec<KnowledgeDocument>, String> {
        let rows = sqlx::query_as::<_, DocRow>(
            "SELECT d.id, d.doc_type, d.title, d.content, d.source, d.tags,
                    d.flow_id, d.task_id, d.subtask_id,
                    d.question, d.description, d.guide_type, d.answer_type, d.code_lang,
                    d.manual, d.created_at
             FROM pent_knowledge_documents d
             WHERE ($2::text IS NULL OR d.doc_type = $2)
               AND ($3::int IS NULL OR d.flow_id = $3)
               AND ($4::int IS NULL OR d.task_id = $4)
               AND to_tsvector('simple', d.title || ' ' || d.content) @@ plainto_tsquery('simple', $1)
             ORDER BY ts_rank(to_tsvector('simple', d.title || ' ' || d.content), plainto_tsquery('simple', $1)) DESC
             LIMIT $5",
        )
        .bind(query)
        .bind(filter.doc_type.as_deref())
        .bind(filter.flow_id)
        .bind(filter.task_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| r.into_doc()).collect())
    }

    pub async fn list(
        &self,
        filter: &KnowledgeFilter,
        limit: usize,
    ) -> Result<Vec<KnowledgeDocument>, String> {
        let rows = sqlx::query_as::<_, DocRow>(
            "SELECT id, doc_type, title, content, source, tags,
                    flow_id, task_id, subtask_id,
                    question, description, guide_type, answer_type, code_lang,
                    manual, created_at
             FROM pent_knowledge_documents
             WHERE ($1::text IS NULL OR doc_type = $1)
               AND ($2::int IS NULL OR flow_id = $2)
               AND ($3::int IS NULL OR task_id = $3)
               AND ($4::int IS NULL OR subtask_id = $4)
             ORDER BY created_at DESC
             LIMIT $5",
        )
        .bind(filter.doc_type.as_deref())
        .bind(filter.flow_id)
        .bind(filter.task_id)
        .bind(filter.subtask_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| r.into_doc()).collect())
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<KnowledgeDocument>, String> {
        let row = sqlx::query_as::<_, DocRow>(
            "SELECT id, doc_type, title, content, source, tags,
                    flow_id, task_id, subtask_id,
                    question, description, guide_type, answer_type, code_lang,
                    manual, created_at
             FROM pent_knowledge_documents WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.map(|r| r.into_doc()))
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, String> {
        sqlx::query("DELETE FROM pent_knowledge_documents WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
            .map_err(|e| e.to_string())
    }

    fn format_vector(v: &[f32]) -> String {
        format!(
            "[{}]",
            v.iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

// ── Filter ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct KnowledgeFilter {
    pub doc_type: Option<String>,
    pub flow_id: Option<i32>,
    pub task_id: Option<i32>,
    pub subtask_id: Option<i32>,
    pub guide_type: Option<String>,
    pub answer_type: Option<String>,
    pub code_lang: Option<String>,
}

// ── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DocRow {
    id: Uuid,
    doc_type: String,
    title: String,
    content: String,
    source: Option<String>,
    tags: Value,
    flow_id: Option<i32>,
    task_id: Option<i32>,
    subtask_id: Option<i32>,
    question: Option<String>,
    description: Option<String>,
    guide_type: Option<String>,
    answer_type: Option<String>,
    code_lang: Option<String>,
    manual: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl DocRow {
    fn into_doc(self) -> KnowledgeDocument {
        KnowledgeDocument {
            id: self.id,
            doc_type: self.doc_type,
            title: self.title,
            content: self.content,
            source: self.source,
            tags: self.tags,
            flow_id: self.flow_id,
            task_id: self.task_id,
            subtask_id: self.subtask_id,
            question: self.question,
            description: self.description,
            answer_type: self.answer_type,
            guide_type: self.guide_type,
            code_lang: self.code_lang,
            manual: self.manual,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SearchRow {
    id: Uuid,
    doc_type: String,
    title: String,
    content: String,
    source: Option<String>,
    tags: Value,
    flow_id: Option<i32>,
    task_id: Option<i32>,
    subtask_id: Option<i32>,
    question: Option<String>,
    description: Option<String>,
    guide_type: Option<String>,
    answer_type: Option<String>,
    code_lang: Option<String>,
    manual: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    similarity: f64,
}

impl SearchRow {
    fn into_result(self) -> KnowledgeSearchResult {
        KnowledgeSearchResult {
            score: self.similarity as f32,
            document: KnowledgeDocument {
                id: self.id,
                doc_type: self.doc_type,
                title: self.title,
                content: self.content,
                source: self.source,
                tags: self.tags,
                flow_id: self.flow_id,
                task_id: self.task_id,
                subtask_id: self.subtask_id,
                question: self.question,
                description: self.description,
                answer_type: self.answer_type,
                guide_type: self.guide_type,
                code_lang: self.code_lang,
                manual: self.manual,
                created_at: self.created_at,
            },
        }
    }
}
