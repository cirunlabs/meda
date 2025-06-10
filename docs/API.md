# Meda REST API Documentation

Meda provides a comprehensive REST API for programmatic VM and image management, similar to Lume's API architecture.

## Getting Started

### Starting the API Server

```bash
# Start on default port (7777) and host (127.0.0.1)
meda serve

# Start on custom port and host
meda serve --port 8080 --host 0.0.0.0

# Start with logging
RUST_LOG=info meda serve
```

### API Documentation

- **Swagger UI**: `http://localhost:7777/swagger-ui`
- **OpenAPI Spec**: `http://localhost:7777/api/v1/openapi.json` (served by Swagger UI)
- **Base URL**: `http://localhost:7777/api/v1`

## Architecture

The API is built using:
- **Axum**: Modern async web framework for Rust
- **OpenAPI/Swagger**: Comprehensive API documentation
- **Tower**: Middleware stack (CORS, tracing, etc.)
- **JSON**: All request/response bodies use JSON format

## Authentication

Currently, the API runs without authentication for local development. For production deployments, consider adding authentication middleware.

## Error Handling

All endpoints return structured error responses:

```json
{
  "error": "Error description",
  "code": "ERROR_CODE",
  "details": {
    "message": "Detailed error information"
  }
}
```

Common HTTP status codes:
- `200`: Success
- `201`: Created successfully
- `400`: Bad request (invalid parameters)
- `404`: Resource not found
- `409`: Conflict (resource already exists)
- `500`: Internal server error

## VM Management API

### List VMs

```http
GET /api/v1/vms
```

**Response:**
```json
{
  "vms": [
    {
      "name": "test-vm",
      "state": "running",
      "ip": "192.168.100.2",
      "memory": "2G",
      "disk": "20G"
    }
  ],
  "count": 1
}
```

### Create VM

```http
POST /api/v1/vms
Content-Type: application/json

{
  "name": "test-vm",
  "user_data": "/path/to/user-data",
  "force": false,
  "memory": "2G",
  "cpus": 4,
  "disk": "20G"
}
```

**Response:**
```json
{
  "success": true,
  "message": "Successfully created VM: test-vm",
  "vm": null
}
```

### Get VM Details

```http
GET /api/v1/vms/{name}
```

**Response:**
```json
{
  "name": "test-vm",
  "state": "running",
  "ip": "192.168.100.2",
  "details": {
    "subnet": "192.168.100",
    "mac": "52:54:00:12:34:56",
    "tap_device": "tap0",
    "memory": "2G",
    "disk_size": "20G",
    "vm_dir": "/home/user/.meda/vms/test-vm"
  }
}
```

### Start VM

```http
POST /api/v1/vms/{name}/start
```

### Stop VM

```http
POST /api/v1/vms/{name}/stop
```

### Get VM IP

```http
GET /api/v1/vms/{name}/ip
```

**Response:**
```json
{
  "vm": "test-vm",
  "ip": "192.168.100.2"
}
```

### Port Forwarding

```http
POST /api/v1/vms/{name}/port-forward
Content-Type: application/json

{
  "host_port": 8080,
  "guest_port": 80
}
```

### Delete VM

```http
DELETE /api/v1/vms/{name}
```

## Image Management API

### List Images

```http
GET /api/v1/images
```

**Response:**
```json
{
  "images": [
    {
      "name": "ubuntu",
      "tag": "latest",
      "registry": "ghcr.io",
      "size": "1.2G",
      "created": "2024-01-15T10:30:00Z"
    }
  ],
  "count": 1
}
```

### Pull Image

```http
POST /api/v1/images/pull
Content-Type: application/json

{
  "image": "ubuntu:latest",
  "registry": "ghcr.io",
  "org": "cirunlabs"
}
```

### Create Image

```http
POST /api/v1/images
Content-Type: application/json

{
  "name": "my-image",
  "tag": "v1.0",
  "registry": "ghcr.io",
  "org": "myorg",
  "from_vm": "test-vm"
}
```

### Push Image

```http
POST /api/v1/images/push
Content-Type: application/json

{
  "name": "my-image",
  "image": "my-registry/my-image:v1.0",
  "registry": "my-registry.com",
  "dry_run": false
}
```

### Run VM from Image

```http
POST /api/v1/images/run
Content-Type: application/json

{
  "image": "ubuntu:latest",
  "name": "api-vm",
  "registry": "ghcr.io",
  "org": "cirunlabs",
  "user_data": "/path/to/user-data",
  "no_start": false,
  "memory": "1G",
  "cpus": 2,
  "disk": "15G"
}
```

### Remove Image

```http
DELETE /api/v1/images/{image}
```

### Prune Images

```http
POST /api/v1/images/prune
Content-Type: application/json

{
  "all": false,
  "force": true
}
```

## Health Check

```http
GET /api/v1/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

## Example Usage

### Create and Start VM via API

```bash
# Create VM
curl -X POST http://localhost:7777/api/v1/vms \
  -H "Content-Type: application/json" \
  -d '{
    "name": "api-test",
    "memory": "2G",
    "cpus": 4,
    "disk": "20G"
  }'

# Start VM
curl -X POST http://localhost:7777/api/v1/vms/api-test/start

# Get VM IP
curl http://localhost:7777/api/v1/vms/api-test/ip

# Set up port forwarding
curl -X POST http://localhost:7777/api/v1/vms/api-test/port-forward \
  -H "Content-Type: application/json" \
  -d '{"host_port": 8080, "guest_port": 80}'
```

### Image Workflow

```bash
# Pull base image
curl -X POST http://localhost:7777/api/v1/images/pull \
  -H "Content-Type: application/json" \
  -d '{"image": "ubuntu:latest"}'

# Run VM from image
curl -X POST http://localhost:7777/api/v1/images/run \
  -H "Content-Type: application/json" \
  -d '{
    "image": "ubuntu:latest",
    "name": "ubuntu-vm",
    "memory": "1G",
    "cpus": 2
  }'

# Create custom image from VM
curl -X POST http://localhost:7777/api/v1/images \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-ubuntu",
    "tag": "v1.0",
    "from_vm": "ubuntu-vm"
  }'
```

## Client Libraries

The API can be consumed by any HTTP client. For JavaScript/TypeScript, Python, Go, or other languages, you can generate client libraries from the OpenAPI specification:

```bash
# Get OpenAPI spec
curl http://localhost:7777/api/v1/openapi.json > meda-api.json

# Generate client libraries using openapi-generator
# Example: Generate TypeScript client
openapi-generator generate -i meda-api.json -g typescript-fetch -o ./meda-client-ts
```

## Production Considerations

For production deployments:

1. **Authentication**: Add authentication middleware
2. **HTTPS**: Use TLS/SSL encryption
3. **Rate Limiting**: Implement rate limiting to prevent abuse
4. **Monitoring**: Add metrics and logging
5. **CORS**: Configure CORS policies appropriately
6. **Validation**: Ensure input validation on all endpoints

## Comparison with Lume API

Meda's API follows similar patterns to Lume:
- REST-based architecture
- JSON request/response format
- Comprehensive Swagger documentation
- VM lifecycle management endpoints
- Image management operations
- Health check endpoint

Key differences:
- Meda uses Axum instead of other web frameworks
- Built-in OpenAPI documentation generation
- Resource specification (memory, CPU, disk) in API calls
- Cloud-Hypervisor integration vs other hypervisors