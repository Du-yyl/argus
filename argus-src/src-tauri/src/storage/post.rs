use diesel::associations::HasTable;
use diesel::prelude::*;
use crate::storage::connection;
use crate::storage::connection::establish_connection;
use crate::storage::schema::posts::dsl::posts;
use crate::storage::schema::posts::published;
use crate::models::post::{NewPost, Post};

/// 获取所有评论
pub fn get_all_post(connection: &mut SqliteConnection) -> Vec<Post> {
    // 指定 published 为 true
    // .filter(published.eq(true))
    let results = posts
        .limit(5)
        .select(Post::as_select())
        .load(connection)
        .expect("Error loading posts");
    results
}

/// 插入评论
pub fn insert_post(conn: &mut SqliteConnection, title: &str, body: &str) -> Post {
    let new_post = NewPost { title, body };

    diesel::insert_into(posts::table())
        .values(&new_post)
        .returning(Post::as_returning())
        .get_result(conn)
        .expect("Error loading posts")
}