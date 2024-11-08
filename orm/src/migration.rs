use async_trait::async_trait;
use sea_orm::{
    sea_query::{ColumnDef, Table},
    DbErr, DeriveIden, DeriveMigrationName,
};
use sea_orm_migration::{
    schema::{boolean, integer, json, pk_auto, string},
    MigrationTrait, SchemaManager,
};

#[derive(DeriveMigrationName, Clone)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(pk_auto(User::Id))
                    .col(string(User::Username))
                    .col(string(User::Password))
                    .col(string(User::Chatacter))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Rule::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Rule::Name).primary_key().not_null().string())
                    .col(string(Rule::Re))
                    .col(integer(Rule::Cost))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Rss::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Rss::Id).primary_key().not_null().string())
                    .col(ColumnDef::new(Rss::Url).string().null())
                    .col(string(Rss::Title))
                    .col(ColumnDef::new(Rss::SearchUrl).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Register::Table)
                    .if_not_exists()
                    .col(pk_auto(Register::Id))
                    .col(string(Register::Code))
                    .col(integer(Register::Timers))
                    .col(integer(Register::Expire))
                    .col(integer(Register::Now))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Config::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Config::Key)
                            .primary_key()
                            .not_null()
                            .string(),
                    )
                    .col(string(Config::Value))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Anime::Table)
                    .if_not_exists()
                    .col(pk_auto(Anime::Id))
                    .col(boolean(Anime::Status))
                    .col(boolean(Anime::IsLock))
                    .col(boolean(Anime::IsSearch))
                    .col(integer(Anime::Progress))
                    .col(json(Anime::AnimeInfo))
                    .col(string(Anime::RuleName))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(AnimeRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AnimeRecord::Title)
                            .primary_key()
                            .not_null()
                            .string(),
                    )
                    .col(integer(AnimeRecord::AnimeId))
                    .col(string(AnimeRecord::Magnet))
                    .col(string(AnimeRecord::RuleName))
                    .col(string(AnimeRecord::InfoHash))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
#[derive(DeriveIden)]
enum AnimeRecord {
    Table,
    Title,
    AnimeId,
    Magnet,
    RuleName,
    InfoHash,
}

#[derive(DeriveIden)]
enum Anime {
    Table,
    Id,
    Status,
    RuleName,
    AnimeInfo,
    IsSearch,
    IsLock,
    Progress,
}

#[derive(DeriveIden)]
enum Config {
    Table,
    Key,
    Value,
}

#[derive(DeriveIden)]
enum Register {
    Table,
    Id,
    Timers,
    Expire,
    Now,
    Code,
}

#[derive(DeriveIden)]
enum Rss {
    Table,
    Id,
    Url,
    Title,
    SearchUrl,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
    Username,
    Password,
    Chatacter,
}

#[derive(DeriveIden)]
enum Rule {
    Table,
    Name,
    Cost,
    Re,
}
