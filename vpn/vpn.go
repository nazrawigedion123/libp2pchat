// // package main

// // import "C" // Required for C-shared libraries
// // import (
// // 	"fmt"
// // 	"log"
// // 	"net/url"

// // 	"github.com/gorilla/websocket"
// // )

// // // The //export comment is strictly required! It tells the Go compiler
// // // to expose this function to C/Rust.
// // //export StartGoVPNClient
// // func StartGoVPNClient(serverURL *C.char) {
// // 	// Convert the C string passed from Rust into a Go string
// // 	address := C.GoString(serverURL)
// // 	fmt.Printf("[Go VPN] Starting background client...\n")
// // 	fmt.Printf("[Go VPN] Connecting to Hub: %s\n", address)

// // 	u, err := url.Parse(address)
// // 	if err != nil {
// // 		log.Printf("[Go VPN Error] Invalid URL: %v", err)
// // 		return
// // 	}

// // 	// Connect to the WebSocket Hub via HTTPS/443 (bypassing firewalls)
// // 	conn, _, err := websocket.DefaultDialer.Dial(u.String(), nil)
// // 	if err != nil {
// // 		log.Printf("[Go VPN Error] Failed to dial cPanel hub: %v", err)
// // 		return
// // 	}
// // 	defer conn.Close()

// // 	fmt.Println("[Go VPN] Successfully connected to the tunnel!")

// // 	// Keep the tunnel alive and listen for incoming data from the hub.
// // 	// In a full implementation, you would pipe this `message` data
// // 	// into a local TCP socket or a TUN interface for libp2p to read.
// // 	for {
// // 		_, message, err := conn.ReadMessage()
// // 		if err != nil {
// // 			log.Println("[Go VPN Error] Connection dropped:", err)
// // 			return
// // 		}
// // 		log.Printf("[Go VPN] Received packet from hub: %s", message)
// // 	}
// // }

// // // main is required by the compiler for c-shared libraries, but left empty.
// // func main() {}

// // package main

// // /*
// // #include <stdint.h>
// // */
// // import "C"

// // import (
// // "fmt"
// // )

// // //export StartVPN
// // func StartVPN() C.int {
// // fmt.Println("VPN Started")
// // return 0
// // }

// // //export StopVPN
// // func StopVPN() C.int {
// // fmt.Println("VPN Stopped")
// // return 0
// // }

// // func main() {}
// package main

// import "C"
// import (
// 	"fmt"
// 	"io"
// 	"net"
// 	"os"
// 	"time"
// )

// //export StartDirectVPNTunnel
// func StartDirectVPNTunnel(localPort C.int, publicListenPort C.int, remoteAddr *C.char) {
// 	localAddrStr := fmt.Sprintf("127.0.0.1:%d", int(localPort))
// 	publicListenStr := fmt.Sprintf("0.0.0.0:%d", int(publicListenPort))
// 	remoteTargetStr := C.GoString(remoteAddr)

// 	fmt.Printf("[Go VPN] Internal Loopback: %s\n", localAddrStr)

// 	// 1. Start listening for the remote peer's incoming internet connection
// 	go func() {
// 		listener, err := net.Listen("tcp", publicListenStr)
// 		if err != nil {
// 			fmt.Fprintf(os.Stderr, "[Go VPN Error] Public listener failed: %v\n", err)
// 			return
// 		}
// 		defer listener.Close()
// 		fmt.Printf("[Go VPN] Listening for remote peer on internet port: %s\n", publicListenStr)

// 		for {
// 			remoteConn, err := listener.Accept()
// 			if err != nil {
// 				continue
// 			}
// 			fmt.Printf("[Go VPN] Remote peer connected from: %s\n", remoteConn.RemoteAddr())
// 			go handleBridge(remoteConn, localAddrStr)
// 		}
// 	}()

// 	// 2. If a remote target IP is explicitly provided, actively attempt to dial out to it
// 	if remoteTargetStr != "" {
// 		go func() {
// 			fmt.Printf("[Go VPN] Actively dialing remote peer: %s\n", remoteTargetStr)
// 			for {
// 				remoteConn, err := net.Dial("tcp", remoteTargetStr)
// 				if err == nil {
// 					fmt.Printf("[Go VPN] Outbound connection established to: %s\n", remoteTargetStr)
// 					handleBridge(remoteConn, localAddrStr)
// 					break
// 				}
// 				// Retry connection if the peer isn't online yet
// 				libVecSleep()
// 			}
// 		}()
// 	}
// }

// // handleBridge links the public internet connection to your local Rust application instance
// func handleBridge(remoteConn net.Conn, localAddr string) {
// 	defer remoteConn.Close()

// 	// Connect to the local Rust app instance running on localhost
// 	localConn, err := net.Dial("tcp", localAddr)
// 	if err != nil {
// 		fmt.Fprintf(os.Stderr, "[Go VPN Error] Rust app on %s is not listening yet: %v\n", localAddr, err)
// 		return
// 	}
// 	defer localConn.Close()

// 	// Split bidirectional traffic streams
// 	chanRemoteToLocal := make(chan struct{})
// 	chanLocalToRemote := make(chan struct{})

// 	go func() {
// 		_, _ = io.Copy(localConn, remoteConn)
// 		close(chanRemoteToLocal)
// 	}()

// 	go func() {
// 		_, _ = io.Copy(remoteConn, localConn)
// 		close(chanLocalToRemote)
// 	}()

// 	// Wait until either side terminates the socket session
// 	select {
// 	case <-chanRemoteToLocal:
// 	case <-chanLocalToRemote:
// 	}
// }

// func libVecSleep() {
// 	select {
// 	case <-typeSleepChan(2):
// 	}
// }

// func typeSleepChan(s int) <-chan javaTime {
// 	c := make(chan javaTime)
// 	go func() {
// 		typeSleep(s)
// 		c <- javaTime{}
// 	}()
// 	return c
// }

// type javaTime struct{}

// func typeSleep(s int) {
// 	// Correct way to use net.DialTimeout
// 	net.DialTimeout("tcp", "127.0.0.1:1", 1*time.Second)

// 	// net.DialTimeout("tcp", "127.0.0.1:1", net.JoinHostPort("", "") /* trigger fake timeout to block safely */)
// 	// Simplified non-blocking fallback generator loop sleep
// }

// func main() {}

package main

import "C"
import (
	"fmt"
	"io"
	"net"
	"time"
)

//export StartDirectVPNTunnel
func StartDirectVPNTunnel(localPort C.int, publicListenPort C.int, remoteAddr *C.char) {
	localAddrStr := fmt.Sprintf("127.0.0.1:%d", int(localPort))
	publicListenStr := fmt.Sprintf("0.0.0.0:%d", int(publicListenPort))
	remoteTargetStr := C.GoString(remoteAddr)

	fmt.Printf("[Go VPN] Internal Loopback Target: %s\n", localAddrStr)

	// 1. Start listening for the remote peer's incoming internet connection
	go func() {
		listener, err := net.Listen("tcp", publicListenStr)
		if err != nil {
			return
		}
		defer listener.Close()
		fmt.Printf("[Go VPN] Listening for remote peer on internet port: %s\n", publicListenStr)

		for {
			remoteConn, err := listener.Accept()
			if err != nil {
				continue
			}
			fmt.Printf("[Go VPN] Remote peer connected from: %s\n", remoteConn.RemoteAddr())
			go handleBridge(remoteConn, localAddrStr)
		}
	}()

	// 2. If a remote target IP is explicitly provided, actively attempt to dial out to it
	if remoteTargetStr != "" {
		go func() {
			fmt.Printf("[Go VPN] Actively dialing remote peer: %s\n", remoteTargetStr)
			for {
				remoteConn, err := net.DialTimeout("tcp", remoteTargetStr, 5*time.Second)
				if err == nil {
					fmt.Printf("[Go VPN] Outbound connection established to: %s\n", remoteTargetStr)
					handleBridge(remoteConn, localAddrStr)
					break
				}
				time.Sleep(1 * time.Second) // Retry dialing remote peer if they aren't up yet
			}
		}()
	}
}

// handleBridge links the public internet connection to your local Rust application instance
func handleBridge(remoteConn net.Conn, localAddr string) {
	defer remoteConn.Close()

	var localConn net.Conn
	var err error

	// Robust Retry Loop: Wait for the Rust application to finish initializing and start listening
	for {
		localConn, err = net.Dial("tcp", localAddr)
		if err == nil {
			break // Successfully linked to Rust!
		}
		// If connection is refused, Rust is still compiling/booting. Wait 200ms and retry.
		time.Sleep(200 * time.Millisecond)
	}
	defer localConn.Close()

	fmt.Printf("[Go VPN] Bridge successfully locked between remote peer and Rust app on %s!\n", localAddr)

	// Split bidirectional traffic streams
	chanRemoteToLocal := make(chan struct{})
	chanLocalToRemote := make(chan struct{})

	go func() {
		_, _ = io.Copy(localConn, remoteConn)
		close(chanRemoteToLocal)
	}()

	go func() {
		_, _ = io.Copy(remoteConn, localConn)
		close(chanLocalToRemote)
	}()

	// Wait until either side terminates the socket session
	select {
	case <-chanRemoteToLocal:
	case <-chanLocalToRemote:
	}
}

func main() {}
