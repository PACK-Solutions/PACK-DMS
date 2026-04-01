# Build & Run – PackDMS

## Prerequisites

- Rust stable (latest)
- Docker & Docker Compose

## Infrastructure

Start PostgreSQL and RustFS:

```bash
docker-compose up -d
```

- **PostgreSQL**: `localhost:5432` (user: `postgres`, password: `password`, db: `packdms`)
- **RustFS S3 API**: `localhost:9000` (credentials: `minioadmin` / `minioadmin`)
- **RustFS Console**: `localhost:9001`

## Environment

Create a `.env` file at the project root:

```env
DATABASE_URL=postgres://postgres:password@localhost:5432/packdms
JWT_ISSUER=https://example.com/auth
JWKS_URL=data/keys/jwks.json
BIND=0.0.0.0:8080
RUST_LOG=info,tower_http=info
S3_ENDPOINT_URL=http://localhost:9000
S3_BUCKET=packdms
S3_REGION=us-east-1
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
```

## Generate Dev Keys

```bash
cargo run --example gen_jwks
```

Creates `data/keys/private.pem` and `data/keys/jwks.json`.

## Build

```bash
cargo build            # debug build
cargo build --release  # release build
```

## Run

```bash
cargo run
```

The API starts at `http://localhost:8080`. Database migrations run automatically on startup.

## API Documentation

- Scalar UI: `http://localhost:8080/docs`
