use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    match command {
        Some("create-user") => {
            if args.len() < 4 {
                eprintln!("Usage: blog_cli create-user <username> <password>");
                std::process::exit(1);
            }
            let (username, password) = (&args[2], &args[3]);

            let db = Database::connect(std::env::var("DATABASE_URL").unwrap())
                .await
                .unwrap();

            let hash = blog::auth::hash_password(password);

            blog::entity::user::ActiveModel {
                username: Set(username.clone()),
                password_hash: Set(hash),
                ..Default::default()
            }
            .insert(&db)
            .await
            .unwrap();

            println!("User '{username}' created.");
        }
        Some("change-password") => {
            if args.len() < 4 {
                eprintln!("Usage: blog_cli change-password <username> <new_password>");
                std::process::exit(1);
            }
            let (username, password) = (&args[2], &args[3]);

            let db = Database::connect(std::env::var("DATABASE_URL").unwrap())
                .await
                .unwrap();

            let user = blog::entity::user::Entity::find()
                .filter(blog::entity::user::Column::Username.eq(username.as_str()))
                .one(&db)
                .await
                .unwrap()
                .unwrap_or_else(|| {
                    eprintln!("User '{username}' not found.");
                    std::process::exit(1);
                });

            let hash = blog::auth::hash_password(password);
            let mut active: blog::entity::user::ActiveModel = user.into();
            active.password_hash = Set(hash);
            active.update(&db).await.unwrap();

            println!("Password changed for '{username}'.");
        }
        _ => {
            eprintln!("Usage: blog_cli <command>");
            eprintln!("Commands:");
            eprintln!("  create-user <username> <password>       Create a user");
            eprintln!("  change-password <username> <password>   Change password");
            std::process::exit(1);
        }
    }
}
