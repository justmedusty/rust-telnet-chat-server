use crate::telnet::{open_telnet_connection, ServerFunctions, TelnetServerConnection};
use std::collections::LinkedList;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

mod telnet;

static PORT: u64 = 6969;

type ConnectionPool = Arc<Mutex<LinkedList<Arc<Mutex<TelnetServerConnection>>>>>;
type Connection = Arc<Mutex<TelnetServerConnection>>;

async fn broadcast_message(message: &Vec<u8>, source: Connection, pool: &ConnectionPool) {
    let pool_ref = if let Ok(x) = pool.lock() {
        x
    } else if let Err(_poisoned) = pool.lock() {
        panic!("lock is poisoned")
    } else {
        unreachable!()
    };
    for connection in pool_ref.iter() {
        if connection.lock().unwrap().connection_id
            == match source.lock() {
                Ok(x) => x,
                Err(_) => continue,
            }
            .connection_id
        {
            continue;
        }
        let mut connection = match connection.lock() {
            Ok(x) => x,
            Err(_) => return,
        };
        connection.fill_write_buffer(message.clone());
        connection.write_to_connection();
        println!("{}", "HERE");
    }
}

fn spawn_server_thread(connection: Connection, pool: ConnectionPool) {
    std::thread::spawn(move   ||{
        let mut conn = connection.lock().unwrap();

        loop {
            println!("Entering thread {}",conn.connection_id);

            let val = conn.read_from_connection();
            if val > 0 {
                println!("Got message on connection {} of size {}", conn.connection_id,val);
                let _ = broadcast_message(&conn.read_buffer, connection.clone(), &pool.clone());
            }
        }
    });
}

fn spawn_connect_thread() {
    std::thread::spawn(move || {
        println!("Starting client thread");
        loop {
            let mut tcp_stream = TcpStream::connect(format!("127.0.0.1:{}", PORT)).unwrap();
            let mut buf = Box::new([0; 4096]);
            tcp_stream
                .write(&Vec::from(String::from("CLIENT SAYS HELLO\n").as_bytes()))
                .expect("Could not write to tcp output stream");
            sleep(Duration::from_secs(2));

            match tcp_stream.read(buf.deref_mut()) {
                Ok(x) => x,
                Err(_) => break,
            };

            println!(
                "Received a message from the client {}",
                String::from_utf8(buf.to_vec()).unwrap()
            );
        }
    });
}

fn main() {
    let mut connection_id = 0;
    let conn_pool = ConnectionPool::new(Mutex::new(Default::default()));
    let server_listener: TcpListener = TcpListener::bind(format!("127.0.0.1:{}", PORT)).unwrap();
    let reference = Arc::new(Mutex::new(server_listener));
    let pool_reference = conn_pool.clone();

    loop {
        let mut pool = match pool_reference.lock() {
            Ok(x) => x,
            Err(_) => panic!("Main thread could not lock mutex"),
        };
        sleep(Duration::from_secs(2));
        let curr = Arc::clone(&reference);
        // Accept the connection and handle it if successful
        spawn_connect_thread();
        let mut server_connection = open_telnet_connection(curr, connection_id);
        println!(
            "Accepted connection from {}",
            server_connection.get_address()
        );
        let reference = Arc::new(Mutex::new(server_connection));
        let unwrapped = Arc::clone(&reference);

        pool.push_front(unwrapped);
        connection_id += 1;

        // Spawn a new thread to handle the connection
        spawn_server_thread(reference.clone(), pool_reference.clone());

        sleep(Duration::new(2, 0));
    }
}
