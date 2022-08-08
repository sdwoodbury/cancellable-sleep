use cancellable_sleep::*;

#[tokio::test]
async fn tes1() {
    println!("beginning test1");
    let mut signal_handler = SignalHandler::init().unwrap();
    let mut ch1 = signal_handler.get_channel();
    let mut ch2 = signal_handler.get_channel();
    let mut ch3 = signal_handler.get_channel();

    let t1 = || async move {
        if ch1.recv_w_timeout(30).await == ChannelCommand::Quit {
            println!("quit received by thread 1");
        }
        println!("thread 1 exiting");
    };

    let t2 = || async move {
        if ch2.recv_w_timeout(30).await == ChannelCommand::Quit {
            println!("quit received by thread 2");
        }
        println!("thread 2 exiting");
    };

    // don't need to pin this because it's not getting passed to join! or select!
    tokio::spawn(async move {
        if ch3.recv_w_timeout(30).await == ChannelCommand::Quit {
            println!("quit received by thread 3");
        }
        println!("thread 3 exiting");
    });

    let t1 = Box::pin(t1());
    let t2 = Box::pin(t2());

    let t3 = Box::pin(signal_handler.await_termination());

    let _ = tokio::join!(t1, t2, t3);
    println!("test1 exiting");
}
