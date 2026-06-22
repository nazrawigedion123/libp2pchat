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
