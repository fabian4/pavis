// Minimal HTTP backend for proxy benchmarking
// Returns a fixed 200 response with minimal processing overhead
package main

import (
	"flag"
	"fmt"
	"log"
	"net/http"
	"time"
)

var (
	port         = flag.String("port", "8000", "Port to listen on")
	responseBody = []byte(`{"status":"ok","backend":"minimal"}`)
)

func main() {
	flag.Parse()

	http.HandleFunc("/", handleRoot)
	http.HandleFunc("/get", handleGet)
	http.HandleFunc("/health", handleHealth)

	addr := ":" + *port
	log.Printf("Minimal backend server starting on %s", addr)
	log.Printf("Response size: %d bytes", len(responseBody))

	server := &http.Server{
		Addr:           addr,
		Handler:        http.DefaultServeMux,
		ReadTimeout:    10 * time.Second,
		WriteTimeout:   10 * time.Second,
		MaxHeaderBytes: 1 << 20,
	}

	if err := server.ListenAndServe(); err != nil {
		log.Fatal(err)
	}
}

func handleRoot(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	w.Write(responseBody)
}

func handleGet(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	w.Write(responseBody)
}

func handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	fmt.Fprint(w, `{"status":"healthy"}`)
}
