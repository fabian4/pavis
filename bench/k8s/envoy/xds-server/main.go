package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"sync"
	"time"

	cluster "github.com/envoyproxy/go-control-plane/envoy/config/cluster/v3"
	core "github.com/envoyproxy/go-control-plane/envoy/config/core/v3"
	endpoint "github.com/envoyproxy/go-control-plane/envoy/config/endpoint/v3"
	listener "github.com/envoyproxy/go-control-plane/envoy/config/listener/v3"
	route "github.com/envoyproxy/go-control-plane/envoy/config/route/v3"
	router "github.com/envoyproxy/go-control-plane/envoy/extensions/filters/http/router/v3"
	hcm "github.com/envoyproxy/go-control-plane/envoy/extensions/filters/network/http_connection_manager/v3"
	"github.com/envoyproxy/go-control-plane/pkg/cache/types"
	"github.com/envoyproxy/go-control-plane/pkg/cache/v3"
	"github.com/envoyproxy/go-control-plane/pkg/resource/v3"
	"github.com/envoyproxy/go-control-plane/pkg/server/v3"
	"github.com/envoyproxy/go-control-plane/pkg/wellknown"
	"google.golang.org/protobuf/types/known/anypb"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/grpc"

	clusterservice "github.com/envoyproxy/go-control-plane/envoy/service/cluster/v3"
	discoverygrpc "github.com/envoyproxy/go-control-plane/envoy/service/discovery/v3"
	endpointservice "github.com/envoyproxy/go-control-plane/envoy/service/endpoint/v3"
	listenerservice "github.com/envoyproxy/go-control-plane/envoy/service/listener/v3"
	routeservice "github.com/envoyproxy/go-control-plane/envoy/service/route/v3"
)

const (
	grpcPort       = 18000
	httpPort       = 8080
	upstreamHost   = "127.0.0.1"
	upstreamPort   = 8081
	nodeID         = "envoy-sidecar"
	clusterName    = "upstream_cluster"
	listenerName   = "listener_0"
	routeConfigName = "local_route"
)

// Minimal xDS server for envoy benchmarking
// Provides LDS, RDS, CDS, EDS for a simple HTTP proxy configuration

type xdsServer struct {
	cache   cache.SnapshotCache
	version int
	mu      sync.Mutex
}

// Callbacks implements the go-control-plane callbacks interface
type callbacks struct{}

func (cb *callbacks) OnStreamOpen(ctx context.Context, id int64, typ string) error {
	log.Printf("Stream opened: id=%d type=%s", id, typ)
	return nil
}

func (cb *callbacks) OnStreamClosed(id int64, node *core.Node) {
	log.Printf("Stream closed: id=%d", id)
}

func (cb *callbacks) OnStreamRequest(id int64, req *discoverygrpc.DiscoveryRequest) error {
	log.Printf("Stream request: id=%d node=%s type=%s version=%s", id, req.Node.Id, req.TypeUrl, req.VersionInfo)
	return nil
}

func (cb *callbacks) OnStreamResponse(ctx context.Context, id int64, req *discoverygrpc.DiscoveryRequest, resp *discoverygrpc.DiscoveryResponse) {
	log.Printf("Stream response: id=%d type=%s version=%s", id, resp.TypeUrl, resp.VersionInfo)
}

func (cb *callbacks) OnFetchRequest(ctx context.Context, req *discoverygrpc.DiscoveryRequest) error {
	log.Printf("Fetch request: node=%s type=%s version=%s", req.Node.Id, req.TypeUrl, req.VersionInfo)
	return nil
}

func (cb *callbacks) OnFetchResponse(req *discoverygrpc.DiscoveryRequest, resp *discoverygrpc.DiscoveryResponse) {
	log.Printf("Fetch response: type=%s version=%s", resp.TypeUrl, resp.VersionInfo)
}

func (cb *callbacks) OnDeltaStreamOpen(ctx context.Context, id int64, typ string) error {
	log.Printf("Delta stream opened: id=%d type=%s", id, typ)
	return nil
}

func (cb *callbacks) OnDeltaStreamClosed(id int64, node *core.Node) {
	log.Printf("Delta stream closed: id=%d", id)
}

func (cb *callbacks) OnStreamDeltaRequest(id int64, req *discoverygrpc.DeltaDiscoveryRequest) error {
	log.Printf("Delta stream request: id=%d node=%s type=%s", id, req.Node.Id, req.TypeUrl)
	return nil
}

func (cb *callbacks) OnStreamDeltaResponse(id int64, req *discoverygrpc.DeltaDiscoveryRequest, resp *discoverygrpc.DeltaDiscoveryResponse) {
	log.Printf("Delta stream response: id=%d type=%s", id, resp.TypeUrl)
}

func newXDSServer() *xdsServer {
	return &xdsServer{
		cache:   cache.NewSnapshotCache(false, cache.IDHash{}, nil),
		version: 1,
	}
}

func (s *xdsServer) makeSnapshot() (*cache.Snapshot, error) {
	// Create cluster pointing to upstream
	upstreamCluster := &cluster.Cluster{
		Name:                 clusterName,
		ConnectTimeout:       durationpb.New(5 * time.Second),
		ClusterDiscoveryType: &cluster.Cluster_Type{Type: cluster.Cluster_STATIC},
		LbPolicy:             cluster.Cluster_ROUND_ROBIN,
		LoadAssignment: &endpoint.ClusterLoadAssignment{
			ClusterName: clusterName,
			Endpoints: []*endpoint.LocalityLbEndpoints{{
				LbEndpoints: []*endpoint.LbEndpoint{{
					HostIdentifier: &endpoint.LbEndpoint_Endpoint{
						Endpoint: &endpoint.Endpoint{
							Address: &core.Address{
								Address: &core.Address_SocketAddress{
									SocketAddress: &core.SocketAddress{
										Protocol: core.SocketAddress_TCP,
										Address:  upstreamHost,
										PortSpecifier: &core.SocketAddress_PortValue{
											PortValue: upstreamPort,
										},
									},
								},
							},
						},
					},
				}},
			}},
		},
	}

	// Create route configuration
	routeConfig := &route.RouteConfiguration{
		Name: routeConfigName,
		VirtualHosts: []*route.VirtualHost{{
			Name:    "local_service",
			Domains: []string{"*"},
			Routes: []*route.Route{{
				Match: &route.RouteMatch{
					PathSpecifier: &route.RouteMatch_Prefix{
						Prefix: "/",
					},
				},
				Action: &route.Route_Route{
					Route: &route.RouteAction{
						ClusterSpecifier: &route.RouteAction_Cluster{
							Cluster: clusterName,
						},
					},
				},
			}},
		}},
	}

	// Create HTTP connection manager with RDS reference
	routerConfig, _ := anypb.New(&router.Router{})
	manager := &hcm.HttpConnectionManager{
		CodecType:  hcm.HttpConnectionManager_AUTO,
		StatPrefix: "ingress_http",
		RouteSpecifier: &hcm.HttpConnectionManager_Rds{
			Rds: &hcm.Rds{
				ConfigSource: &core.ConfigSource{
					ResourceApiVersion: core.ApiVersion_V3,
					ConfigSourceSpecifier: &core.ConfigSource_Ads{
						Ads: &core.AggregatedConfigSource{},
					},
				},
				RouteConfigName: routeConfigName,
			},
		},
		HttpFilters: []*hcm.HttpFilter{{
			Name: wellknown.Router,
			ConfigType: &hcm.HttpFilter_TypedConfig{
				TypedConfig: routerConfig,
			},
		}},
	}

	managerAny, _ := anypb.New(manager)

	// Create listener
	listenerConfig := &listener.Listener{
		Name: listenerName,
		Address: &core.Address{
			Address: &core.Address_SocketAddress{
				SocketAddress: &core.SocketAddress{
					Protocol: core.SocketAddress_TCP,
					Address:  "0.0.0.0",
					PortSpecifier: &core.SocketAddress_PortValue{
						PortValue: 8080,
					},
				},
			},
		},
		FilterChains: []*listener.FilterChain{{
			Filters: []*listener.Filter{{
				Name: wellknown.HTTPConnectionManager,
				ConfigType: &listener.Filter_TypedConfig{
					TypedConfig: managerAny,
				},
			}},
		}},
	}

	// Create snapshot
	snapshot, err := cache.NewSnapshot(
		fmt.Sprintf("%d", s.version),
		map[resource.Type][]types.Resource{
			resource.ClusterType:  {upstreamCluster},
			resource.RouteType:    {routeConfig},
			resource.ListenerType: {listenerConfig},
			resource.EndpointType: {},
		},
	)
	if err != nil {
		return nil, err
	}

	// Validate snapshot consistency
	if err := snapshot.Consistent(); err != nil {
		log.Printf("Snapshot inconsistency: %v", err)
		return nil, fmt.Errorf("snapshot inconsistent: %w", err)
	}

	return snapshot, nil
}

func (s *xdsServer) updateSnapshot() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.version++
	snapshot, err := s.makeSnapshot()
	if err != nil {
		log.Printf("Failed to create snapshot: %v", err)
		return fmt.Errorf("failed to create snapshot: %w", err)
	}

	log.Printf("Setting snapshot for node %s with version %d", nodeID, s.version)
	if err := s.cache.SetSnapshot(context.Background(), nodeID, snapshot); err != nil {
		log.Printf("Failed to set snapshot: %v", err)
		return fmt.Errorf("failed to set snapshot: %w", err)
	}

	log.Printf("Updated snapshot to version %d", s.version)
	return nil
}

func (s *xdsServer) handlePublish(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	if err := s.updateSnapshot(); err != nil {
		log.Printf("Failed to update snapshot: %v", err)
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"status":  "ok",
		"version": s.version,
	})
}

func (s *xdsServer) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{
		"status": "healthy",
	})
}

func main() {
	ctx := context.Background()

	// Create xDS server
	xds := newXDSServer()

	// Initialize with first snapshot
	if err := xds.updateSnapshot(); err != nil {
		log.Fatalf("Failed to create initial snapshot: %v", err)
	}

	// Create gRPC server
	grpcServer := grpc.NewServer()

	// Register xDS services with callbacks
	// The server.NewServer returns an implementation of all xDS services
	cb := &callbacks{}
	srv := server.NewServer(ctx, xds.cache, cb)
	discoverygrpc.RegisterAggregatedDiscoveryServiceServer(grpcServer, srv)
	endpointservice.RegisterEndpointDiscoveryServiceServer(grpcServer, srv)
	clusterservice.RegisterClusterDiscoveryServiceServer(grpcServer, srv)
	routeservice.RegisterRouteDiscoveryServiceServer(grpcServer, srv)
	listenerservice.RegisterListenerDiscoveryServiceServer(grpcServer, srv)

	grpcListener, err := net.Listen("tcp", fmt.Sprintf(":%d", grpcPort))
	if err != nil {
		log.Fatalf("Failed to listen on gRPC port: %v", err)
	}

	go func() {
		log.Printf("Starting gRPC xDS server on :%d", grpcPort)
		if err := grpcServer.Serve(grpcListener); err != nil {
			log.Fatalf("gRPC server failed: %v", err)
		}
	}()

	// Start HTTP API server
	http.HandleFunc("/v1/publish", xds.handlePublish)
	http.HandleFunc("/health", xds.handleHealth)

	log.Printf("Starting HTTP API server on :%d", httpPort)
	if err := http.ListenAndServe(fmt.Sprintf(":%d", httpPort), nil); err != nil {
		log.Fatalf("HTTP server failed: %v", err)
	}
}
