use axum::Json;
use serde_json::{Value, json};

pub async fn document() -> Json<Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Kairos Platform Gateway API",
            "version": "0.1.0"
        },
        "servers": [{ "url": "/api" }],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }
            }
        },
        "paths": {
            "/system/capabilities": { "get": { "tags": ["system"], "summary": "System module capabilities", "responses": { "200": { "description": "OK" } } } },
            "/system/auth/login": { "post": { "tags": ["auth"], "summary": "Login", "responses": { "200": { "description": "Token pair" }, "401": { "description": "Invalid credentials" } } } },
            "/system/auth/refresh-token": { "get": { "tags": ["auth"], "summary": "Refresh access token", "responses": { "200": { "description": "Rotated token pair" }, "401": { "description": "Invalid refresh token" } } } },
            "/system/auth/logout": { "post": { "tags": ["auth"], "summary": "Logout and revoke refresh token", "responses": { "200": { "description": "OK" } } } },
            "/system/auth/me": { "get": { "tags": ["auth"], "summary": "Current user", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Current user permissions" } } } },
            "/system/users": {
                "get": { "tags": ["system-user"], "summary": "List users", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Users" } } },
                "post": { "tags": ["system-user"], "summary": "Create user", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Created user id" } } }
            },
            "/system/users/{id}": {
                "put": { "tags": ["system-user"], "summary": "Update user", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } },
                "delete": { "tags": ["system-user"], "summary": "Delete user", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } }
            },
            "/system/users/{id}/roles": { "put": { "tags": ["system-user"], "summary": "Assign user roles", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } } },
            "/system/roles": {
                "get": { "tags": ["system-role"], "summary": "List roles", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Roles" } } },
                "post": { "tags": ["system-role"], "summary": "Create role", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Created role id" } } }
            },
            "/system/roles/{id}": {
                "put": { "tags": ["system-role"], "summary": "Update role", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } },
                "delete": { "tags": ["system-role"], "summary": "Delete role", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } }
            },
            "/system/roles/{id}/permissions": { "put": { "tags": ["system-role"], "summary": "Assign role permissions", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } } },
            "/system/permissions": { "get": { "tags": ["system-permission"], "summary": "List permissions", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Permissions" } } } },
            "/system/audit-logs": { "get": { "tags": ["system-audit"], "summary": "List audit logs", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Audit logs" } } } },
            "/health": { "get": { "tags": ["ops"], "summary": "Health check", "responses": { "200": { "description": "OK" } } } },
            "/infra/capabilities": { "get": { "tags": ["infra"], "summary": "Infra module capabilities", "responses": { "200": { "description": "OK" } } } },
            "/media/capabilities": { "get": { "tags": ["media"], "summary": "Media module capabilities", "responses": { "200": { "description": "OK" } } } },
            "/media/assets": {
                "get": { "tags": ["media"], "summary": "List media assets", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Assets" } } },
                "post": { "tags": ["media"], "summary": "Create media asset", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Created asset id" } } }
            },
            "/media/assets/{id}": {
                "get": { "tags": ["media"], "summary": "Get media asset", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "Asset" } } },
                "put": { "tags": ["media"], "summary": "Update media asset", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } },
                "delete": { "tags": ["media"], "summary": "Delete media asset", "security": [{ "bearerAuth": [] }], "responses": { "200": { "description": "OK" } } }
            }
        }
    }))
}
