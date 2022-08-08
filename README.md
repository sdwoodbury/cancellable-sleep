# cancellable-sleep
This library allows threads to sleep until a timer elapses or a shutdown signal is received.
This is especially useful for daemon processes which may sleep for minutes, where the programmer desires that the sleeping process recieve the signal immediately.

Obtain channels via `get_channel`.
Perform the "cancellable sleep" via `recv_w_timeout`
Create an async task to catch the signals via `await_termination`