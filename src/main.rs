extern crate time;
extern crate iron;
extern crate persistent;
extern crate router;
extern crate hyper;
extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;

extern crate handlebars_iron;
extern crate rustc_serialize;

/// Standard lib crates
use std::env;
use std::net::*;

use time::precise_time_ns;

// Json crates
use rustc_serialize::json;

// Iron crates
use iron::prelude::*;
use iron::status;
use iron::typemap::Key;
use iron::{BeforeMiddleware, AfterMiddleware, typemap};
use router::Router;
use persistent::{Read};
use hyper::header::{Headers, ContentType};
use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};

// Postgres crates
use r2d2::{Pool, PooledConnection};
use r2d2_postgres::{TlsMode, PostgresConnectionManager};

// Types
pub type PostgresPool = Pool<PostgresConnectionManager>;
pub type PostgresPooledConnection = PooledConnection<PostgresConnectionManager>;

// Log Time
struct ResponseTime;
impl typemap::Key for ResponseTime { type Value = u64; }

// AppDB
pub struct AppDb;
impl Key for AppDb { type Value = PostgresPool; }

#[derive(RustcDecodable, RustcEncodable)]
struct Order {
  id          : i32,
  number      : String,
  reference   : String,
  status      : i32,
  notes       : String,
  price       : i32,
  merchant_id : String,
  uuid        : String
}

impl Default for Order {
    fn default() -> Order {
        Order {
            id          : 0,
            number      : "".to_string(),
            reference   : "".to_string(),
            status      : 0,
            notes       : "".to_string(),
            price       : 0,
            merchant_id : "".to_string(),
            uuid        : "".to_string()
        }
    }
}

// Filtro executado no inicio da requisicao
impl BeforeMiddleware for ResponseTime {
    fn before(&self, req: &mut Request) -> IronResult<()> {
        req.extensions.insert::<ResponseTime>(precise_time_ns());
        Ok(())
    }
}

// Filtro executado no final da requisicao
impl AfterMiddleware for ResponseTime {
    fn after(&self, req: &mut Request, res: Response) -> IronResult<Response> {
        let delta = precise_time_ns() - *req.extensions.get::<ResponseTime>().unwrap();
        println!("Request took: {} ms", (delta as f64) / 1000000.0);
        Ok(res)
    }
}

// Helper methods
fn setup_connection_pool(cn_str: &str, pool_size: u32) -> PostgresPool {
    let manager = ::r2d2_postgres::PostgresConnectionManager::new(cn_str, TlsMode::None).unwrap();
    let config = ::r2d2::Config::builder().pool_size(pool_size).build();
    ::r2d2::Pool::new(config, manager).unwrap()
}

fn database(req: &mut Request) -> IronResult<Response> {
    let pool = req.get::<Read<AppDb>>().unwrap();
    let order_id = req.extensions.get::<Router>().unwrap().find("order_id").unwrap_or("none");
    let conn = pool.get().unwrap();
    let mut data = Order::default();
    println!("Order {}", &order_id);
    for row in &conn.query("select * from orders where uuid = $1", &[&order_id]).unwrap() {
          data = Order {
              id          : row.get("id"),
              number      : row.get("number"),
              reference   : row.get("reference"),
              status      : row.get("status"),
              notes       : row.get("notes"),
              price       : row.get("price"),
              merchant_id : row.get("merchant_id"),
              uuid        : row.get("uuid")
          }; break;
    }
    let encoded = json::encode(&data).unwrap();
    let mut response = Response::new();
    response.set_mut(status::Ok);
    response.set_mut(encoded);
    response.headers.set(ContentType(Mime(TopLevel::Application, SubLevel::Json,vec![(Attr::Charset, Value::Utf8)])));
    Ok(response)
}

fn handler(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "OK")))
}

fn main() {

    let conn_string:String = match env::var("DATABASE_URL") {
        Ok(val) => val,
        Err(_) => "postgres://postgres:admin@localhost:5432/omaha_order_manager_development".to_string()
    };

    let conn_pool:String = match env::var("DATABASE_POOL") {
        Ok(val) => val,
        Err(_) => "2".to_string()
    };

    let pool_size: u32 = conn_pool.parse::<u32>().unwrap();

    println!("connecting to postgres: {}", conn_string);
    let pool = setup_connection_pool(&conn_string, pool_size);

    let mut router = Router::new();
    router.get("/", handler, "handler");
    router.get("/api/v2/orders/:order_id", database, "showOrder");

    let mut chain = Chain::new(router);
    chain.link(Read::<AppDb>::both(pool));
    chain.link_before(ResponseTime);
    chain.link_after(ResponseTime);

    let host = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 8085);
    println!("listening on http://{}", host);
    Iron::new(chain).http(host).unwrap();
}
