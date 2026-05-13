export async function up(knex: any, db: any) {
  await knex.schema.createTable("users", (table: any) => {
    table.increments("id");
    table.string("email");
  });

  await knex.schema.alterTable("users", (table: any) => {
    table.string("name");
  });

  db.insert("users", { email: "a@example.com" });
  db.select("*").from("users");
  db.raw("ALTER TABLE users ADD COLUMN age integer");
}
