# Insighta Labs+ Web Portal

A web portal for the Insighta Labs+ platform, providing profile management, analytics dashboards, and natural language search through a browser interface. Built with Next.js 14, TypeScript, and Tailwind CSS.

## Features

- GitHub OAuth authentication via HTTP-only cookies
- Dashboard with profile metrics (total, male, female counts)
- Filterable and paginated profiles list with CSV export
- Profile detail views
- Natural language search
- Real-time data reflection from the same backend APIs used by the CLI
- CSRF protection via SameSite cookie attribute

## Tech Stack

- [Next.js 14](https://nextjs.org) (App Router)
- [TypeScript](https://www.typescriptlang.org)
- [Tailwind CSS](https://tailwindcss.com)

## Pages

| Route | Description |
| --- | --- |
| `/` | Landing page with GitHub OAuth login button |
| `/login` | Login page |
| `/dashboard` | Dashboard displaying profile metrics (total, male, female counts) |
| `/profiles` | Profiles list with filters (gender, age group, country), pagination, and CSV export |
| `/profiles/[id]` | Profile detail view |
| `/search` | Natural language search page |
| `/account` | User account info with logout |
| `/auth/callback` | OAuth callback handler |

## Authentication

Authentication is handled via GitHub OAuth with secure token management:

- Access and refresh tokens are stored in **HTTP-only cookies**, making them inaccessible to JavaScript (`document.cookie` cannot read them).
- **CSRF protection** is enforced through the `SameSite` cookie attribute.
- All API requests include the `X-API-Version: 1` header, matching the same backend APIs used by the CLI.
- Data is reflected in real time from the backend.

## Getting Started

### Prerequisites

- Node.js 18 or later
- A running instance of the [Insighta backend API](https://github.com/DavidIfebueme/come-test-api)

### Install and Run

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

### Other Commands

```bash
npm run build    # Production build
npm run start    # Start production server
npm run lint     # Run linter
```

## Configuration

### Environment Variables

| Variable | Description | Default |
| --- | --- | --- |
| `NEXT_PUBLIC_API_URL` | Backend API URL | `http://localhost:8080` |

Create a `.env.local` file in the project root to override defaults:

```
NEXT_PUBLIC_API_URL=http://localhost:8080
```

## Repositories

- **Web (this repo):** [insighta-web](https://github.com/DavidIfebueme/insighta-web)
- **Backend API:** [come-test-api](https://github.com/DavidIfebueme/come-test-api)
- **CLI:** [insighta-cli](https://github.com/DavidIfebueme/insighta-cli)
