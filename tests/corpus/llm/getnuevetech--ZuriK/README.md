# African Fashion Marketplace API

A privacy-first African fashion marketplace backend built with NestJS, connecting Designers, Fabric Sellers, Customers, QA, and Admin.

## 🚀 Phase 1: Backend Foundation

This is Phase 1 of the phased development plan, establishing the backend foundation:

- **Authentication**: JWT-based auth (register, login, refresh, profile)
- **User Management**: Full CRUD for users with role-based access
- **Database**: PostgreSQL with TypeORM
- **API Docs**: Swagger at `/docs`

## 🏗️ Tech Stack

- **Framework**: NestJS 10
- **Language**: TypeScript
- **Database**: PostgreSQL 15 with TypeORM
- **Auth**: JWT (access + refresh tokens) with Passport
- **Validation**: class-validator + class-transformer
- **Docs**: Swagger / OpenAPI

## 🐳 Running with Docker Compose

```bash
# Copy environment variables
cp .env.example .env

# Start services
docker compose up --build

# API available at http://localhost:3000
# Swagger docs at http://localhost:3000/docs
```

## 🌱 Running Seeds

```bash
# Development (ts-node)
npm run seed

# Production (after build)
npm run seed:prod
```

Seeds create one user per role:
- `admin@africanfashion.com` — Admin
- `designer@africanfashion.com` — Designer
- `seller@africanfashion.com` — Fabric Seller
- `qa@africanfashion.com` — QA
- `customer@africanfashion.com` — Customer

All with password: `Password123!`

## 📦 Available Scripts

- `npm run build` — Build for production
- `npm start` — Start production server
- `npm run start:dev` — Start development server with hot reload
- `npm run seed` — Seed database (development)
- `npm run seed:prod` — Seed database (production)
- `npm run typeorm:generate -- src/database/migrations/MigrationName` — Generate a new migration
- `npm run typeorm:run` — Run pending migrations
- `npm run typeorm:revert` — Revert the last migration

## 🔑 Environment Variables

See `.env.example` for required variables:

```
DATABASE_URL=postgresql://postgres:password@localhost:5432/african_fashion_db
JWT_ACCESS_SECRET=your-access-secret
JWT_REFRESH_SECRET=your-refresh-secret
NODE_ENV=development
PORT=3000
```

## 🔄 Changing Public URLs

When migrating to a new domain or URL, **no code changes are needed** — just update environment variables:

1. **Vercel (Frontend):**
   - `NEXT_PUBLIC_API_URL` → your backend URL (e.g., `https://api.yourdomain.com`)
   - `NEXT_PUBLIC_APP_URL` → your frontend URL (e.g., `https://yourdomain.com`)

2. **Railway (Backend):**
   - `FRONTEND_URL` → your frontend URL (used for CORS, email links)
   - `DATABASE_URL` → your database connection string

3. **External Services:**
   - Update payment provider webhook URLs
   - Update OAuth callback URLs (if applicable)
   - Update DNS records (if domain changes)

4. **Redeploy both services** — changes take effect automatically.

> ⚠️ The app validates required env vars at startup in production. If any are missing, it will fail fast with a clear error message instead of silently falling back to localhost.

## 🗄️ Database Migrations

Schema synchronisation (`synchronize`) is **disabled in production** (`NODE_ENV=production`).
In development it remains enabled for convenience. For production deployments always use migrations:

```bash
# Generate a new migration after changing entities
npm run typeorm:generate -- src/database/migrations/DescriptiveName

# Apply all pending migrations
npm run typeorm:run

# Revert the last applied migration
npm run typeorm:revert
```

Migration files are stored in `src/database/migrations/` and should be committed to version control.
The DataSource configuration lives in `src/database/typeorm.config.ts`.

## 📚 API Documentation

Swagger docs available at `http://localhost:3000/docs` after starting the server.

### Phase 1 Endpoints

**Authentication** (`/auth`):
- `POST /auth/register` — Register new user
- `POST /auth/login` — Login
- `POST /auth/refresh` — Refresh tokens
- `GET /auth/profile` — Get current user (requires JWT)

**Users** (`/users`):
- `GET /users` — List all users
- `GET /users/:id` — Get user by ID
- `PATCH /users/:id` — Update user
- `DELETE /users/:id` — Deactivate user

## 🗺️ Development Phases

- **Phase 1** ✅ — Backend Foundation (Auth + User Management)
- **Phase 2** 🔜 — Products & Fabrics
- **Phase 3** 🔜 — Orders & Payments
- **Phase 4** 🔜 — QA Workflow
- **Phase 5** 🔜 — Notifications & Admin Dashboard

## 📄 License

This project is licensed under the [MIT License](./LICENSE).
