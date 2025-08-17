use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedIssue {
    pub id: i32,
    pub project_id: i32,
    pub project_name: String,
    pub subject: String,
    pub description: Option<String>,
    pub status_id: i32,
    pub status_name: String,
    pub priority_id: i32,
    pub priority_name: String,
    pub assigned_to_id: Option<i32>,
    pub assigned_to_name: Option<String>,
    pub created_on: DateTime<Utc>,
    pub updated_on: DateTime<Utc>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedProject {
    pub id: i32,
    pub name: String,
    pub identifier: String,
    pub description: Option<String>,
    pub is_public: bool,
    pub parent_id: Option<i32>,
    pub created_on: DateTime<Utc>,
    pub updated_on: DateTime<Utc>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedUser {
    pub id: i32,
    pub login: String,
    pub firstname: String,
    pub lastname: String,
    pub mail: Option<String>,
    pub created_on: DateTime<Utc>,
    pub last_login_on: Option<DateTime<Utc>>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedTimeEntry {
    pub id: i32,
    pub project_id: i32,
    pub project_name: String,
    pub issue_id: Option<i32>,
    pub user_id: i32,
    pub user_name: String,
    pub activity_id: i32,
    pub activity_name: String,
    pub hours: f64,
    pub comments: Option<String>,
    pub spent_on: String,
    pub created_on: DateTime<Utc>,
    pub updated_on: DateTime<Utc>,
    pub cached_at: DateTime<Utc>,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(format!("Failed to create directory: {}", e)),
                )
            })?;
        }

        let conn = Connection::open(db_path)?;
        
        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        
        let db = Database {
            conn: Mutex::new(conn),
        };
        
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Create issues table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cached_issues (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL,
                project_name TEXT NOT NULL,
                subject TEXT NOT NULL,
                description TEXT,
                status_id INTEGER NOT NULL,
                status_name TEXT NOT NULL,
                priority_id INTEGER NOT NULL,
                priority_name TEXT NOT NULL,
                assigned_to_id INTEGER,
                assigned_to_name TEXT,
                created_on TEXT NOT NULL,
                updated_on TEXT NOT NULL,
                cached_at TEXT NOT NULL
            )",
            [],
        )?;

        // Create projects table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cached_projects (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                identifier TEXT NOT NULL UNIQUE,
                description TEXT,
                is_public INTEGER NOT NULL,
                parent_id INTEGER,
                created_on TEXT NOT NULL,
                updated_on TEXT NOT NULL,
                cached_at TEXT NOT NULL
            )",
            [],
        )?;

        // Create users table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cached_users (
                id INTEGER PRIMARY KEY,
                login TEXT NOT NULL UNIQUE,
                firstname TEXT NOT NULL,
                lastname TEXT NOT NULL,
                mail TEXT,
                created_on TEXT NOT NULL,
                last_login_on TEXT,
                cached_at TEXT NOT NULL
            )",
            [],
        )?;

        // Create time entries table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cached_time_entries (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL,
                project_name TEXT NOT NULL,
                issue_id INTEGER,
                user_id INTEGER NOT NULL,
                user_name TEXT NOT NULL,
                activity_id INTEGER NOT NULL,
                activity_name TEXT NOT NULL,
                hours REAL NOT NULL,
                comments TEXT,
                spent_on TEXT NOT NULL,
                created_on TEXT NOT NULL,
                updated_on TEXT NOT NULL,
                cached_at TEXT NOT NULL
            )",
            [],
        )?;

        // Create indexes for better query performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_issues_project_id ON cached_issues(project_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_issues_assigned_to_id ON cached_issues(assigned_to_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_issues_status_id ON cached_issues(status_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_time_entries_project_id ON cached_time_entries(project_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_time_entries_issue_id ON cached_time_entries(issue_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_time_entries_user_id ON cached_time_entries(user_id)",
            [],
        )?;

        Ok(())
    }

    // Issue operations
    pub fn cache_issue(&self, issue: &CachedIssue) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO cached_issues 
            (id, project_id, project_name, subject, description, status_id, status_name, 
             priority_id, priority_name, assigned_to_id, assigned_to_name, 
             created_on, updated_on, cached_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                issue.id,
                issue.project_id,
                issue.project_name,
                issue.subject,
                issue.description,
                issue.status_id,
                issue.status_name,
                issue.priority_id,
                issue.priority_name,
                issue.assigned_to_id,
                issue.assigned_to_name,
                issue.created_on.to_rfc3339(),
                issue.updated_on.to_rfc3339(),
                issue.cached_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_cached_issue(&self, id: i32) -> Result<Option<CachedIssue>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, project_name, subject, description, status_id, status_name,
                    priority_id, priority_name, assigned_to_id, assigned_to_name,
                    created_on, updated_on, cached_at
             FROM cached_issues WHERE id = ?1"
        )?;

        let issue = stmt.query_row([id], |row| {
            Ok(CachedIssue {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                subject: row.get(3)?,
                description: row.get(4)?,
                status_id: row.get(5)?,
                status_name: row.get(6)?,
                priority_id: row.get(7)?,
                priority_name: row.get(8)?,
                assigned_to_id: row.get(9)?,
                assigned_to_name: row.get(10)?,
                created_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                    .unwrap()
                    .with_timezone(&Utc),
                cached_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        }).optional()?;

        Ok(issue)
    }

    pub fn get_cached_issues(&self, project_id: Option<i32>, limit: usize) -> Result<Vec<CachedIssue>> {
        let conn = self.conn.lock().unwrap();
        
        let query = if let Some(pid) = project_id {
            format!(
                "SELECT id, project_id, project_name, subject, description, status_id, status_name,
                        priority_id, priority_name, assigned_to_id, assigned_to_name,
                        created_on, updated_on, cached_at
                 FROM cached_issues 
                 WHERE project_id = {}
                 ORDER BY updated_on DESC
                 LIMIT {}",
                pid, limit
            )
        } else {
            format!(
                "SELECT id, project_id, project_name, subject, description, status_id, status_name,
                        priority_id, priority_name, assigned_to_id, assigned_to_name,
                        created_on, updated_on, cached_at
                 FROM cached_issues 
                 ORDER BY updated_on DESC
                 LIMIT {}",
                limit
            )
        };

        let mut stmt = conn.prepare(&query)?;
        let issue_iter = stmt.query_map([], |row| {
            Ok(CachedIssue {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                subject: row.get(3)?,
                description: row.get(4)?,
                status_id: row.get(5)?,
                status_name: row.get(6)?,
                priority_id: row.get(7)?,
                priority_name: row.get(8)?,
                assigned_to_id: row.get(9)?,
                assigned_to_name: row.get(10)?,
                created_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                    .unwrap()
                    .with_timezone(&Utc),
                cached_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })?;

        let mut issues = Vec::new();
        for issue in issue_iter {
            issues.push(issue?);
        }

        Ok(issues)
    }

    // Project operations
    pub fn cache_project(&self, project: &CachedProject) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO cached_projects 
            (id, name, identifier, description, is_public, parent_id, 
             created_on, updated_on, cached_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project.id,
                project.name,
                project.identifier,
                project.description,
                project.is_public,
                project.parent_id,
                project.created_on.to_rfc3339(),
                project.updated_on.to_rfc3339(),
                project.cached_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_cached_projects(&self, limit: usize) -> Result<Vec<CachedProject>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            &format!(
                "SELECT id, name, identifier, description, is_public, parent_id,
                        created_on, updated_on, cached_at
                 FROM cached_projects 
                 ORDER BY name
                 LIMIT {}",
                limit
            )
        )?;

        let project_iter = stmt.query_map([], |row| {
            Ok(CachedProject {
                id: row.get(0)?,
                name: row.get(1)?,
                identifier: row.get(2)?,
                description: row.get(3)?,
                is_public: row.get(4)?,
                parent_id: row.get(5)?,
                created_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                    .unwrap()
                    .with_timezone(&Utc),
                cached_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })?;

        let mut projects = Vec::new();
        for project in project_iter {
            projects.push(project?);
        }

        Ok(projects)
    }

    // User operations
    pub fn cache_user(&self, user: &CachedUser) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO cached_users 
            (id, login, firstname, lastname, mail, created_on, last_login_on, cached_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                user.id,
                user.login,
                user.firstname,
                user.lastname,
                user.mail,
                user.created_on.to_rfc3339(),
                user.last_login_on.map(|d| d.to_rfc3339()),
                user.cached_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    // Time entry operations
    pub fn cache_time_entry(&self, entry: &CachedTimeEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO cached_time_entries 
            (id, project_id, project_name, issue_id, user_id, user_name, 
             activity_id, activity_name, hours, comments, spent_on, 
             created_on, updated_on, cached_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.id,
                entry.project_id,
                entry.project_name,
                entry.issue_id,
                entry.user_id,
                entry.user_name,
                entry.activity_id,
                entry.activity_name,
                entry.hours,
                entry.comments,
                entry.spent_on,
                entry.created_on.to_rfc3339(),
                entry.updated_on.to_rfc3339(),
                entry.cached_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    // Search operations
    pub fn search_issues(
        &self, 
        query: &str,
        project_id: Option<i32>,
        status_id: Option<i32>,
        assigned_to_id: Option<i32>,
        limit: usize,
        offset: usize
    ) -> Result<Vec<CachedIssue>> {
        let conn = self.conn.lock().unwrap();
        
        let mut sql = String::from(
            "SELECT id, project_id, project_name, subject, description, status_id, status_name,
                    priority_id, priority_name, assigned_to_id, assigned_to_name,
                    created_on, updated_on, cached_at
             FROM cached_issues 
             WHERE 1=1"
        );
        
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        // Add search query condition if provided
        if !query.is_empty() {
            sql.push_str(" AND (subject LIKE ?1 OR description LIKE ?1)");
            params.push(Box::new(format!("%{}%", query)));
        }
        
        // Add filter conditions
        if let Some(pid) = project_id {
            sql.push_str(&format!(" AND project_id = {}", pid));
        }
        
        if let Some(sid) = status_id {
            sql.push_str(&format!(" AND status_id = {}", sid));
        }
        
        if let Some(aid) = assigned_to_id {
            sql.push_str(&format!(" AND assigned_to_id = {}", aid));
        }
        
        sql.push_str(&format!(" ORDER BY updated_on DESC LIMIT {} OFFSET {}", limit, offset));
        
        let mut stmt = conn.prepare(&sql)?;
        
        let row_to_issue = |row: &rusqlite::Row| -> Result<CachedIssue> {
            Ok(CachedIssue {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                subject: row.get(3)?,
                description: row.get(4)?,
                status_id: row.get(5)?,
                status_name: row.get(6)?,
                priority_id: row.get(7)?,
                priority_name: row.get(8)?,
                assigned_to_id: row.get(9)?,
                assigned_to_name: row.get(10)?,
                created_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_on: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                    .unwrap()
                    .with_timezone(&Utc),
                cached_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        };
        
        let issue_iter = if !query.is_empty() {
            stmt.query_map([&format!("%{}%", query)], row_to_issue)?
        } else {
            stmt.query_map([], row_to_issue)?
        };
        
        let mut issues = Vec::new();
        for issue in issue_iter {
            issues.push(issue?);
        }
        
        Ok(issues)
    }

    // Clear cache operations
    pub fn clear_all_cache(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM cached_issues", [])?;
        conn.execute("DELETE FROM cached_projects", [])?;
        conn.execute("DELETE FROM cached_users", [])?;
        conn.execute("DELETE FROM cached_time_entries", [])?;
        Ok(())
    }

    pub fn clear_old_cache(&self, days: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let cutoff_date = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        
        conn.execute("DELETE FROM cached_issues WHERE cached_at < ?1", [&cutoff_date])?;
        conn.execute("DELETE FROM cached_projects WHERE cached_at < ?1", [&cutoff_date])?;
        conn.execute("DELETE FROM cached_users WHERE cached_at < ?1", [&cutoff_date])?;
        conn.execute("DELETE FROM cached_time_entries WHERE cached_at < ?1", [&cutoff_date])?;
        
        Ok(())
    }

    // Get cache statistics
    pub fn get_cache_stats(&self) -> Result<serde_json::Value> {
        let conn = self.conn.lock().unwrap();
        
        let issue_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM cached_issues",
            [],
            |row| row.get(0),
        )?;
        
        let project_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM cached_projects",
            [],
            |row| row.get(0),
        )?;
        
        let user_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM cached_users",
            [],
            |row| row.get(0),
        )?;
        
        let time_entry_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM cached_time_entries",
            [],
            |row| row.get(0),
        )?;
        
        Ok(serde_json::json!({
            "issues": issue_count,
            "projects": project_count,
            "users": user_count,
            "time_entries": time_entry_count,
            "total": issue_count + project_count + user_count + time_entry_count
        }))
    }
}