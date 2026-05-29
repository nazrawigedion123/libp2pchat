# Define the target and its prerequisite (source file)
libgovpn.a: vpn.go
	go build -buildmode=c-archive -o libgovpn.a vpn.go

# Add a clean rule to easily remove the generated library and its header
.PHONY: clean
clean:
	rm -f libgovpn.a libgovpn.h
