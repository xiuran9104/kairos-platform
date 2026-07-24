"""Kairos Security Range — 本地漏洞靶场.

专为测试平台的 AI 渗透测试、资产扫描、漏洞检测能力设计。
包含 OWASP Top 10 常见漏洞类型。
"""

import hashlib
import json
import os
import sqlite3
import subprocess
from pathlib import Path

from flask import Flask, Response, jsonify, redirect, request, session

app = Flask(__name__)
app.secret_key = "weak-secret-key-vulnerable-app-2024"

# ── Database ─────────────────────────────────────────────────────────────────
DB_PATH = Path(__file__).parent / "vuln.db"


def get_db() -> sqlite3.Connection:
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    return conn


def init_db() -> None:
    conn = get_db()
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            email TEXT,
            role TEXT DEFAULT 'user',
            credit_card TEXT,
            ssn TEXT
        );
        CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS comments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER,
            author TEXT,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT,
            price REAL,
            description TEXT
        );
        """
    )
    # Seed data
    existing = conn.execute("SELECT COUNT(*) FROM users").fetchone()[0]
    if existing == 0:
        conn.execute(
            "INSERT INTO users(username, password, email, role, credit_card, ssn) "
            "VALUES(?,?,?,?,?,?)",
            ("admin", "Admin#123456", "admin@kairos.local", "admin",
             "4111-1111-1111-1111", "123-45-6789"),
        )
        conn.execute(
            "INSERT INTO users(username, password, email, role) "
            "VALUES(?,?,?,?)",
            ("alice", "password123", "alice@example.com", "user"),
        )
        conn.execute(
            "INSERT INTO users(username, password, email, role) "
            "VALUES(?,?,?,?)",
            ("bob", "qwerty", "bob@example.com", "user"),
        )
        conn.execute(
            "INSERT INTO posts(user_id, title, content) VALUES(?,?,?)",
            (1, "Welcome to Kairos Security Range",
             "This is a deliberately vulnerable application for security testing."),
        )
        conn.execute(
            "INSERT INTO posts(user_id, title, content) VALUES(?,?,?)",
            (2, "Internal API Keys",
             "Please do not share: AWS_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE, "
             "STRIPE_SECRET=sk_live_abc123def456"),
        )
        conn.execute(
            "INSERT INTO comments(post_id, author, content) VALUES(?,?,?)",
            (1, "anonymous", "First comment! <script>alert('XSS')</script>"),
        )
        conn.execute(
            "INSERT INTO products(name, price, description) VALUES(?,?,?)",
            ("Flag: FLAG{Kairos_SecRange_2024}", 9999.99,
             "Congratulations! You found the hidden flag."),
        )
    conn.commit()
    conn.close()


# ── Helpers ──────────────────────────────────────────────────────────────────


def auth_required():
    """Vulnerable: weak auth check, susceptible to session manipulation."""
    return session.get("user_id") is not None


def current_user_id():
    return session.get("user_id")


def is_admin():
    return session.get("role") == "admin"


# ═══════════════════════════════════════════════════════════════════════════════
# VULNERABLE ENDPOINTS
# ═══════════════════════════════════════════════════════════════════════════════

# ── A1: Broken Access Control ─────────────────────────────────────────────────


@app.route("/")
def index():
    return """
    <h1>Kairos Security Range</h1>
    <p>A deliberately vulnerable application for AI-driven pentest testing.</p>
    <ul>
        <li><a href="/login">Login</a></li>
        <li><a href="/register">Register</a></li>
        <li><a href="/posts">Blog Posts</a></li>
        <li><a href="/products">Products</a></li>
        <li><a href="/ping">Ping Tool</a></li>
        <li><a href="/fetch">URL Fetcher</a></li>
        <li><a href="/debug">Debug Info</a></li>
        <li><a href="/admin">Admin Panel</a></li>
    </ul>
    """


@app.route("/register", methods=["GET", "POST"])
def register():
    if request.method == "POST":
        username = request.form.get("username", "")
        password = request.form.get("password", "")
        email = request.form.get("email", "")
        conn = get_db()
        conn.execute(
            "INSERT INTO users(username, password, email) VALUES(?,?,?)",
            (username, password, email),
        )
        conn.commit()
        conn.close()
        return "Registered! <a href='/login'>Login</a>"
    return """
    <h2>Register</h2>
    <form method="post">
        <input name="username" placeholder="Username"><br>
        <input name="password" type="password" placeholder="Password"><br>
        <input name="email" placeholder="Email"><br>
        <button type="submit">Register</button>
    </form>
    """


@app.route("/login", methods=["GET", "POST"])
def login():
    if request.method == "POST":
        username = request.form.get("username", "")
        password = request.form.get("password", "")

        # VULN: SQL Injection in login
        conn = get_db()
        query = (
            f"SELECT id, username, role FROM users "
            f"WHERE username='{username}' AND password='{password}'"
        )
        try:
            row = conn.execute(query).fetchone()
        except Exception:
            conn.close()
            return f"<h2>Database Error</h2><pre>{query}</pre>"
        conn.close()

        if row:
            session["user_id"] = row["id"]
            session["username"] = row["username"]
            session["role"] = row["role"]
            return redirect("/profile")
        return f"Login failed. <pre>{query}</pre>"
    return """
    <h2>Login</h2>
    <form method="post">
        <input name="username" placeholder="Username"><br>
        <input name="password" type="password" placeholder="Password"><br>
        <button type="submit">Login</button>
    </form>
    <p><small>Hint: try admin / Admin#123456</small></p>
    """


@app.route("/logout")
def logout():
    session.clear()
    return redirect("/")


# VULN: IDOR — no access control on user profiles
@app.route("/users/<int:user_id>")
def user_profile(user_id):
    conn = get_db()
    # VULN: No authorization check — anyone can view any user
    row = conn.execute(
        "SELECT id, username, email, role, credit_card, ssn, password FROM users "
        "WHERE id=?",
        (user_id,),
    ).fetchone()
    conn.close()
    if row:
        return jsonify(dict(row))
    return "User not found", 404


@app.route("/profile")
def profile():
    if not auth_required():
        return redirect("/login")
    return user_profile(current_user_id())


# VULN: Admin panel with weak access control
@app.route("/admin")
def admin_panel():
    # VULN: role check via easily manipulated session
    if not auth_required():
        return redirect("/login")
    conn = get_db()
    users = conn.execute("SELECT id, username, email, role FROM users").fetchall()
    conn.close()
    html = "<h2>Admin Panel</h2><table border='1'>"
    html += "<tr><th>ID</th><th>Username</th><th>Email</th><th>Role</th></tr>"
    for u in users:
        html += (
            f"<tr><td>{u['id']}</td><td>{u['username']}</td>"
            f"<td>{u['email']}</td><td>{u['role']}</td></tr>"
        )
    html += "</table>"
    if not is_admin():
        html += "<p style='color:red'>Access denied: admin role required</p>"
    return html


# ── A2: Cryptographic Failures ────────────────────────────────────────────────


# VULN: Exposed sensitive debug endpoint
@app.route("/debug")
def debug_info():
    """VULN: Exposes internal configuration."""
    return jsonify(
        {
            "app_secret": app.secret_key,
            "database_path": str(DB_PATH),
            "env_vars": dict(os.environ),
            "python_version": os.sys.version,
        }
    )


# VULN: robots.txt exposes sensitive paths
@app.route("/robots.txt")
def robots():
    return Response(
        "User-agent: *\nDisallow: /admin\nDisallow: /debug\n"
        "Disallow: /backup\nDisallow: /api/internal\n",
        mimetype="text/plain",
    )


# VULN: Backup endpoint — exposes database
@app.route("/backup")
def backup():
    """VULN: Exposes full database download."""
    with open(DB_PATH, "rb") as f:
        return Response(f.read(), mimetype="application/octet-stream",
                        headers={"Content-Disposition": "attachment; filename=vuln.db"})


# ── A3: Injection ────────────────────────────────────────────────────────────


# VULN: SQL Injection — search endpoint
@app.route("/posts")
def list_posts():
    search = request.args.get("search", "")
    conn = get_db()
    if search:
        # VULN: SQL injection in search
        query = (
            f"SELECT p.id, p.title, p.content, u.username "
            f"FROM posts p LEFT JOIN users u ON p.user_id=u.id "
            f"WHERE p.title LIKE '%{search}%' OR p.content LIKE '%{search}%'"
        )
    else:
        query = (
            "SELECT p.id, p.title, p.content, u.username "
            "FROM posts p LEFT JOIN users u ON p.user_id=u.id"
        )
    try:
        rows = conn.execute(query).fetchall()
    except Exception as e:
        conn.close()
        return f"<h2>Error</h2><pre>{query}</pre><p>{e}</p>"
    conn.close()
    html = "<h2>Blog Posts</h2><form><input name='search' placeholder='Search...'><button>Search</button></form>"
    for row in rows:
        html += f"<div><h3>{row['title']}</h3><p>{row['content']}</p><small>by {row['username']}</small></div>"
        html += f"<a href='/posts/{row['id']}'>Read more</a><hr>"
    return html


@app.route("/posts/<int:post_id>")
def view_post(post_id):
    conn = get_db()
    row = conn.execute(
        "SELECT p.*, u.username FROM posts p LEFT JOIN users u ON p.user_id=u.id "
        "WHERE p.id=?",
        (post_id,),
    ).fetchone()
    # VULN: Stored XSS — comments rendered unsanitized
    comments = conn.execute(
        "SELECT * FROM comments WHERE post_id=? ORDER BY created_at DESC",
        (post_id,),
    ).fetchall()
    conn.close()
    if not row:
        return "Not found", 404
    html = f"<h2>{row['title']}</h2><p>{row['content']}</p><small>by {row['username']}</small>"
    html += "<hr><h3>Comments</h3>"
    for c in comments:
        # VULN: Stored XSS — no HTML escaping
        html += f"<div><strong>{c['author']}</strong>: {c['content']}<br></div>"
    html += """
    <hr><h4>Add Comment</h4>
    <form method="post" action="/comments">
        <input name="post_id" type="hidden" value="{}">
        <input name="author" placeholder="Name"><br>
        <textarea name="content" placeholder="Comment"></textarea><br>
        <button type="submit">Submit</button>
    </form>
    """.format(post_id)
    return html


@app.route("/comments", methods=["POST"])
def add_comment():
    post_id = request.form.get("post_id", "0")
    author = request.form.get("author", "anonymous")
    content = request.form.get("content", "")
    conn = get_db()
    # VULN: Stored XSS — unsanitized input stored directly
    conn.execute(
        "INSERT INTO comments(post_id, author, content) VALUES(?,?,?)",
        (post_id, author, content),
    )
    conn.commit()
    conn.close()
    return redirect(f"/posts/{post_id}")


# VULN: Command Injection
@app.route("/ping", methods=["GET", "POST"])
def ping():
    result = ""
    if request.method == "POST":
        host = request.form.get("host", "127.0.0.1")
        # VULN: Command injection via unsanitized input
        try:
            result = subprocess.check_output(
                f"ping -c 3 {host}", shell=True, stderr=subprocess.STDOUT, timeout=10
            ).decode()
        except subprocess.TimeoutExpired:
            result = "Timeout"
        except Exception as e:
            result = str(e)
    return f"""
    <h2>Ping Tool</h2>
    <form method="post">
        <input name="host" placeholder="Host or IP" value="127.0.0.1">
        <button type="submit">Ping</button>
    </form>
    <pre>{result}</pre>
    <small>Example: 127.0.0.1; whoami</small>
    """


# ── A5: Security Misconfiguration ────────────────────────────────────────────


# VULN: CORS misconfiguration
@app.after_request
def add_cors(response):
    # VULN: overly permissive CORS
    response.headers["Access-Control-Allow-Origin"] = "*"
    response.headers["Access-Control-Allow-Methods"] = "GET, POST, PUT, DELETE, OPTIONS"
    response.headers["Access-Control-Allow-Headers"] = "*"
    return response


# VULN: Missing security headers
@app.after_request
def missing_headers(response):
    # VULN: intentionally NOT setting security headers
    # No X-Frame-Options, no Content-Security-Policy, no X-Content-Type-Options
    return response


# ── SSRF ─────────────────────────────────────────────────────────────────────


# VULN: Server-Side Request Forgery
@app.route("/fetch", methods=["GET", "POST"])
def fetch_url():
    content = ""
    if request.method == "POST":
        url = request.form.get("url", "")
        # VULN: SSRF — no URL validation
        try:
            import urllib.request
            with urllib.request.urlopen(url, timeout=5) as resp:
                content = resp.read().decode(errors="replace")[:2000]
        except Exception as e:
            content = str(e)
    return f"""
    <h2>URL Fetcher</h2>
    <form method="post">
        <input name="url" placeholder="https://example.com" size="40">
        <button type="submit">Fetch</button>
    </form>
    <pre>{content}</pre>
    <small>Try: http://127.0.0.1:8080/health or file:///etc/passwd</small>
    """


# ── File Path Traversal ──────────────────────────────────────────────────────


@app.route("/files")
def file_viewer():
    filename = request.args.get("name", "")
    if not filename:
        return "<h2>File Viewer</h2><p>Usage: /files?name=app.py</p>"
    # VULN: Path traversal
    base = Path(__file__).parent
    filepath = base / filename
    try:
        content = filepath.read_text()
        return f"<h2>{filename}</h2><pre>{content}</pre>"
    except Exception as e:
        return str(e)


# ── Open Redirect ────────────────────────────────────────────────────────────


@app.route("/redirect")
def open_redirect():
    target = request.args.get("to", "/")
    # VULN: Open redirect — no validation
    return redirect(target)


# ── API Endpoints (for REST testing) ─────────────────────────────────────────


@app.route("/api/products")
def api_products():
    conn = get_db()
    rows = conn.execute("SELECT id, name, price, description FROM products").fetchall()
    conn.close()
    return jsonify([dict(r) for r in rows])


@app.route("/api/products/<int:product_id>")
def api_product(product_id):
    conn = get_db()
    # VULN: SQL Injection via URL parameter (if ORM was not used)
    row = conn.execute(
        f"SELECT id, name, price, description FROM products WHERE id={product_id}"
    ).fetchone()
    conn.close()
    if row:
        return jsonify(dict(row))
    return "Not found", 404


# VULN: JWT-like token with weak signing
@app.route("/api/auth", methods=["POST"])
def api_auth():
    data = request.get_json(force=True, silent=True) or {}
    username = data.get("username", "")
    password = data.get("password", "")
    conn = get_db()
    # VULN: SQL injection
    row = conn.execute(
        f"SELECT id, username, role FROM users WHERE username='{username}' "
        f"AND password='{password}'"
    ).fetchone()
    conn.close()
    if row:
        # VULN: Weak token — just base64 of user data, no signature
        import base64
        payload = json.dumps(dict(row))
        token = base64.b64encode(payload.encode()).decode()
        return jsonify({"token": token, "user": dict(row)})
    return jsonify({"error": "Invalid credentials"}), 401


# ── Information Disclosure ───────────────────────────────────────────────────


@app.route("/api/internal/users")
def internal_users():
    """VULN: No authentication required for sensitive endpoint."""
    conn = get_db()
    rows = conn.execute(
        "SELECT id, username, email, password, role, credit_card, ssn FROM users"
    ).fetchall()
    conn.close()
    return jsonify([dict(r) for r in rows])


@app.errorhandler(404)
def not_found(e):
    return jsonify({
        "error": "not found",
        "path": request.path,
        "method": request.method,
        "available_endpoints": [rule.rule for rule in app.url_map.iter_rules()]
    }), 404


@app.errorhandler(500)
def server_error(e):
    # VULN: Verbose error with traceback
    import traceback
    return f"<h2>500 Internal Server Error</h2><pre>{traceback.format_exc()}</pre>", 500


# ═══════════════════════════════════════════════════════════════════════════════

if __name__ == "__main__":
    init_db()
    print("🔴 Kairos Security Range starting on http://0.0.0.0:8888")
    print("   Vulnerable target ready for AI pentest.")
    app.run(host="0.0.0.0", port=8888, debug=True)
