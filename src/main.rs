use std::num::TryFromIntError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::types::Json;
use sqlx::PgPool;
use sqlx::Pool;
use sqlx::Postgres;
use sqlx::Row;

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "JOB_STATUS")]
enum JobStatus {
    Queued,
    Running,
    Failed,
}

#[derive(Serialize, Deserialize, Debug)]
enum Payload {
    NOOP,
    SendEmail { email: String },
}

#[derive(Serialize, Deserialize, Debug)]
enum Params {
    NOOP,
    FollowUp(bool),
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
    status: JobStatus,
    payload: Json<Payload>,
    params: Option<Json<Params>>,
}

#[derive(Debug)]
#[allow(dead_code)] // see dbg! at the end of main
struct DomainJob {
    identifier: String,
    status: JobStatus,
    payload: Payload,
}

impl TryFrom<JobRow> for DomainJob {
    type Error = TryFromIntError;

    fn try_from(value: JobRow) -> Result<Self, Self::Error> {
        let nid = u32::try_from(value.id)?;
        let job = DomainJob {
            identifier: format!("BATCH({})", nid/3),
            status: value.status,
            payload: value.payload.0,
        };
        Ok(job)
    }
}

async fn must_get_pool() -> Pool<Postgres> {
    PgPool::connect("postgres://postgres:leak-ok-123@localhost:5433/my_app")
        .await
        .expect("Could not connect to the database!")
}

fn insert_jobs() -> Query<'static, Postgres, PgArguments> {
    println!("Inserting jobs...");
    sqlx::query!(
        r#"
        INSERT INTO jobs (status, payload, params)
        VALUES ($1, $2, NULL)
             , ($1, $3, $4)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, $5)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, $6)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
             , ($1, $2, NULL)
             , ($1, $3, NULL)
    "#,
        JobStatus::Queued as JobStatus,
        json!(Payload::NOOP),
        json!(Payload::SendEmail {
            email: "user@example.com".to_string()
        }),
        json!(Params::NOOP),
        json!(Params::FollowUp(true)),
        json!(Params::FollowUp(false)),
    )
}

#[tokio::main]
async fn main() {
    let pg_pool = must_get_pool().await;

    let mut domain_jobs: Vec<DomainJob> = vec!();

    insert_jobs()
        .execute(&pg_pool)
        .await
        .expect("Could not insert");

    println!("1) ==> `query_as!`");
    println!(
        "1) ==> Use SQL type override to fix this error: '{}'",
        r#"error: unsupported type job_status of column #2 ("status")"#
    );
    let jobs = sqlx::query_as!(
        JobRow,
        r#"
            UPDATE jobs
            SET status = 'Running'
            WHERE id IN (
                SELECT id
                FROM jobs
                WHERE status = 'Queued'
                ORDER BY id
                LIMIT 5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, status AS "status: JobStatus", payload AS "payload: Json<Payload>", params AS "params: Json<Params>"
            "#
    )
    .fetch_all(&pg_pool)
    .await
    .expect("failed to grab jobs!");

    for job in jobs {
        println!(
            "1) Working on job #{} ({:?}) -> {:?} | {:?}",
            job.id, job.status, job.payload, job.params,
        );

        work_on_payload(&job.payload.0);

        let domain_job: DomainJob = job.try_into().expect("could not construct DomainJob");
        domain_jobs.push(domain_job);
    }

    println!();
    println!("2) ==> `query_as`");
    println!("2) ==> this requires the `sqlx::FromRow` trait AND specifying the containing variable type (`Vec<Job>`)");
    let jobs: Vec<JobRow> = sqlx::query_as(
        r#"
            UPDATE jobs
            SET status = 'Running'
            WHERE id IN (
                SELECT id
                FROM jobs
                WHERE status = 'Queued'
                ORDER BY id
                LIMIT 5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, status, payload, params
            "#,
    )
    .fetch_all(&pg_pool)
    .await
    .expect("failed to grab jobs!");

    for job in jobs {
        println!(
            "2) Working on job #{} ({:?}) -> {:?} | {:?}",
            job.id, job.status, job.payload, job.params
        );
        work_on_payload(&job.payload.0);
    }

    println!();
    println!("3) ==> `query!`");
    println!("3) ==> this requires the `sqlx::FromRow` trait AND the SQL type override");
    let records = sqlx::query!(
        r#"
            UPDATE jobs
            SET status = 'Running'
            WHERE id IN (
                SELECT id
                FROM jobs
                WHERE status = 'Queued'
                ORDER BY id
                LIMIT 5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, status AS "status: JobStatus", payload, params
            "#
    )
    .fetch_all(&pg_pool)
    .await
    .expect("failed to grab jobs!");

    for record in records {
        println!(
            "3) Working on job #{} ({:?}) -> {:?} | {:?}",
            record.id, record.status, record.payload, record.params
        );
        work_on_payload(&serde_json::from_value(record.payload).unwrap())
    }

    println!();
    println!("4) ==> `query`");
    println!("4) ==> No requirements (manual conversion)");
    let pg_rows = sqlx::query(
        r#"
            UPDATE jobs
            SET status = 'Running'
            WHERE id IN (
                SELECT id
                FROM jobs
                WHERE status = 'Queued'
                ORDER BY id
                LIMIT 5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, status, payload, params
            "#,
    )
    .fetch_all(&pg_pool)
    .await
    .expect("failed to grab rows!");

    for row in pg_rows {
        let id: i64 = row.try_get("id").unwrap();
        let status: JobStatus = row.try_get("status").unwrap();
        let payload: Json<Payload> = row.try_get("payload").unwrap();
        let params: Option<Json<Params>> = row.try_get("params").unwrap();
        println!(
            "4) Working on job #{} ({:?}) -> {:?} | {:?}",
            id, status, payload, params
        );
        work_on_payload(&payload);
    }

    println!("======================");
    println!("Domain jobs conversion!");
    println!("======================");
    dbg!(domain_jobs);

    ()
}

fn work_on_payload(payload: &Payload) {
    match payload {
        Payload::NOOP => println!("   --- NOOP!"),
        Payload::SendEmail { email } => {
            println!("   --- EMAIL[{}]", email.to_ascii_uppercase());
        }
    }
}
