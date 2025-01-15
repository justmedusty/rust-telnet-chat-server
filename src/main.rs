use crate::telnet::{
    open_telnet_connection, ServerFunctions, TelnetServerConnection, VALID_CONNECTION,
};
use std::collections::VecDeque;
use std::fmt::Display;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::{Add, DerefMut};
use std::sync::{Arc, RwLock};

mod telnet;

static PORT: u64 = 6969;

static GREETING: &'static str = "Welcome to the server, what will your username be? ";
static INVALID_NAME: &'static str = "That is not a valid username. What will your username be? ";
static SUCCESS_STRING: &'static str = "Username is valid, joining session\n";

type ConnectionPool = Arc<RwLock<VecDeque<Connection>>>;
type Connection = Arc<RwLock<TelnetServerConnection>>;

fn broadcast_message(message: &Vec<u8>, source: u64, pool: &ConnectionPool) {
    println!("broadcasting message from {}", source);
    {
        let pool_ref = pool.read().unwrap();

        for connection in pool_ref.iter() {
            let mut dest: u64 = 0;

            let mut conn = match connection.write() {
                Ok(x) => x,
                Err(_) => continue,
            };

            if conn.connection_id == source {
                continue;
            }
            dest = conn.connection_id;
            println!("Sending message to dest: {}", dest);
            conn.write_from_passed_buffer(message);
        }
    }
}
fn handle_new_connection(connection: Connection, pool: ConnectionPool) {
    let mut greeted = false;
    let mut username: String = "".to_string();
    loop {
        let mut conn = connection.write().unwrap();
        conn.flush_write_buffer();
        if !greeted {
            conn.fill_write_buffer(Vec::from(GREETING.clone().trim().as_bytes()));
            conn.write_to_connection();
            conn.flush_write_buffer();
        }
        greeted = true;
        let length = conn.read_from_connection_blocking();
        if length == 0 {
            return;
        }
        let name = String::from_utf8_lossy(&*conn.read_buffer.to_vec())
            .trim()
            .to_string();
        conn.flush_read_buffer();

        println!("New connection: {}", name);

        if (length > 4 && length < 25) {
            conn.flush_write_buffer();
            conn.fill_write_buffer(Vec::from(SUCCESS_STRING.clone()));
            conn.write_to_connection();
            conn.flush_write_buffer();
            let mut name = name.clone();
            name.truncate(length as usize);
            conn.set_name(name);
            username = conn.name.clone();
            break;
        }

        conn.flush_write_buffer();
        conn.fill_write_buffer(Vec::from(INVALID_NAME.clone()));
        conn.write_to_connection();
    }
    let count;
    {
        let mut pool_ref = pool.write().unwrap();
        count = pool_ref.iter().count();
        pool_ref.insert(count, connection.clone());
    }

    {
        let mut conn = connection.write().unwrap();
        conn.connection_id = count as u64;
    }

    let message = format!("{} has joined\n", username);
    let message_vec = message.into_bytes();
    broadcast_message(&message_vec, count as u64, &pool);

    return;
}

fn spawn_server_thread(connection: Connection, pool: ConnectionPool) {
    std::thread::spawn(move || loop {
        let (mut read_buffer, mut connection_id, mut val);
        handle_new_connection(connection.clone(), pool.clone());
        loop {
            {
                let mut conn = match connection.write() {
                    Ok(x) => x,
                    Err(_) => break,
                };

                connection_id = conn.connection_id;
                val = conn.read_from_connection();

                if val > 0 && val != VALID_CONNECTION as usize {
                    read_buffer = conn.read_buffer.clone();
                    read_buffer.resize(val as usize, 0);
                    let mut prefix = conn.name.clone().into_bytes();
                    prefix.push(b':');
                    prefix.push(b' ');
                    prefix.extend_from_slice(&read_buffer);
                    prefix.push(b'\n');
                    read_buffer = prefix;
                } else if val == VALID_CONNECTION as usize {
                    continue;
                } else if val == 0 {
                    {
                        let mut pool = pool.write().unwrap();
                        println!("Connection {} closed", conn.connection_id);
                        pool.remove(conn.connection_id as usize);
                    }
                    let message = format!("{} has left\n", conn.name);
                    let message_vec = message.into_bytes();
                    broadcast_message(&message_vec, conn.connection_id, &pool);

                    return;
                } else {
                    continue;
                }
            }
            broadcast_message(&read_buffer, connection_id, &pool);

            {
                let mut conn = match connection.write() {
                    Ok(x) => x,
                    Err(_) => break,
                };
                conn.flush_read_buffer();
                conn.flush_write_buffer();
            }
        }
    });
}
/*
This function is just for testing purposes
*/
fn spawn_connect_thread() {
    std::thread::spawn(move || loop {
        println!("Starting client thread");
        let mut tcp_stream = TcpStream::connect(format!("127.0.0.1:{}", PORT)).unwrap();
        let mut buf = Box::new([0; 1024]);
        tcp_stream
            .write(&Vec::from(String::from("TESTBOT\n").as_bytes()))
            .expect("Could not write to tcp output stream");
        loop {
            tcp_stream.read(buf.deref_mut()).unwrap();
            if (buf[0] != b'\0') {
                println!(
                    "Received a message from the server : {}",
                    String::from_utf8(buf.to_vec()).unwrap()
                );

                buf.iter_mut().for_each(|x| *x = 0);
            }
        }
    });
}

fn main() {
    let conn_pool = ConnectionPool::new(RwLock::new(Default::default()));
    let server_listener: TcpListener = TcpListener::bind(format!("127.0.0.1:{}", PORT)).unwrap();
    let reference = Arc::new(RwLock::new(server_listener));
    let pool_reference = Arc::clone(&conn_pool);

    loop {
        let curr = Arc::clone(&reference);

        let mut server_connection = open_telnet_connection(curr, 0 /* Placeholder value */);

        println!(
            "Accepted connection from {}",
            server_connection.get_address()
        );
        let reference = Arc::new(RwLock::new(server_connection));
        let unwrapped = Arc::clone(&reference);

        // Spawn a new thread to handle the connection

        spawn_server_thread(Arc::clone(&unwrapped), Arc::clone(&pool_reference));
    }
}
