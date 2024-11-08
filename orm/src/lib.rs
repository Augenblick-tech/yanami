use async_trait::async_trait;
use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod migration;
pub mod sea_orm;
pub mod sqlx;

pub struct Migrator;

#[async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(migration::Migration)]
    }
}
