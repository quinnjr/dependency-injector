// Syntax-highlighted code snippets for documentation

export const CODE_SNIPPETS = {
  // Installation
  install: `<span class="text-slate-500">[dependencies]</span>
<span class="text-rust-400">dependency-injector</span> = <span class="text-green-400">"2"</span>`,

  features: `<span class="text-slate-500">[dependencies]</span>
<span class="text-rust-400">dependency-injector</span> = { version = <span class="text-green-400">"2"</span>, features = [<span class="text-green-400">"async"</span>] }

<span class="text-slate-500"># Or disable default features</span>
<span class="text-rust-400">dependency-injector</span> = { version = <span class="text-green-400">"2"</span>, default-features = <span class="text-purple-400">false</span> }`,

  // Quick Start / Home Example
  example: `<span class="text-purple-400">use</span> dependency_injector::Container;

<span class="text-slate-500">// Define your services</span>
<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Database</span> { url: <span class="text-blue-400">String</span> }

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">UserService</span> { db: Database }

<span class="text-purple-400">fn</span> <span class="text-blue-400">main</span>() {
    <span class="text-slate-500">// Create container</span>
    <span class="text-purple-400">let</span> container = Container::new();

    <span class="text-slate-500">// Register services</span>
    container.singleton(Database {
        url: <span class="text-green-400">"postgres://localhost"</span>.into()
    });

    <span class="text-slate-500">// Lazy initialization</span>
    <span class="text-purple-400">let</span> c = container.clone();
    container.lazy(<span class="text-purple-400">move</span> || UserService {
        db: c.get().unwrap()
    });

    <span class="text-slate-500">// Resolve anywhere</span>
    <span class="text-purple-400">let</span> users = container.get::&lt;UserService&gt;().unwrap();
}`,

  // Quick Start Full
  quickStart: `<span class="text-purple-400">use</span> dependency_injector::Container;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Database</span> {
    url: <span class="text-blue-400">String</span>,
}

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">UserService</span> {
    db: Database,
}

<span class="text-purple-400">fn</span> <span class="text-blue-400">main</span>() {
    <span class="text-slate-500">// Create a new container</span>
    <span class="text-purple-400">let</span> container = Container::new();

    <span class="text-slate-500">// Register a singleton service</span>
    container.singleton(Database {
        url: <span class="text-green-400">"postgres://localhost/mydb"</span>.into(),
    });

    <span class="text-slate-500">// Register a service with a factory</span>
    <span class="text-purple-400">let</span> c = container.clone();
    container.lazy(<span class="text-purple-400">move</span> || UserService {
        db: c.get().unwrap(),
    });

    <span class="text-slate-500">// Resolve services</span>
    <span class="text-purple-400">let</span> db = container.get::&lt;Database&gt;().unwrap();
    <span class="text-purple-400">let</span> users = container.get::&lt;UserService&gt;().unwrap();

    println!(<span class="text-green-400">"Connected to: {}"</span>, db.url);
}`,

  // Lifetimes
  lifetimes: `<span class="text-purple-400">use</span> dependency_injector::Container;
<span class="text-purple-400">use</span> std::sync::atomic::{AtomicU64, Ordering};

<span class="text-purple-400">static</span> COUNTER: AtomicU64 = AtomicU64::new(<span class="text-yellow-400">0</span>);

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Config</span> { debug: <span class="text-purple-400">bool</span> }

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">RequestId</span>(<span class="text-blue-400">u64</span>);

<span class="text-purple-400">let</span> container = Container::new();

<span class="text-slate-500">// Singleton - created immediately, shared everywhere</span>
container.singleton(Config { debug: <span class="text-purple-400">true</span> });

<span class="text-slate-500">// Lazy singleton - created on first access</span>
container.lazy(|| Config { debug: <span class="text-purple-400">false</span> });

<span class="text-slate-500">// Transient - new instance every time</span>
container.transient(|| {
    RequestId(COUNTER.fetch_add(<span class="text-yellow-400">1</span>, Ordering::SeqCst))
});`,

  // Scopes
  scopes: `<span class="text-purple-400">use</span> dependency_injector::Container;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">AppConfig</span> { name: <span class="text-blue-400">String</span> }

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">RequestContext</span> { id: <span class="text-blue-400">String</span> }

<span class="text-slate-500">// Root container with app-wide services</span>
<span class="text-purple-400">let</span> root = Container::new();
root.singleton(AppConfig { name: <span class="text-green-400">"MyApp"</span>.into() });

<span class="text-slate-500">// Per-request scope - inherits from root</span>
<span class="text-purple-400">let</span> request_scope = root.scope();
request_scope.singleton(RequestContext { id: <span class="text-green-400">"req-123"</span>.into() });

<span class="text-slate-500">// Request scope can access root services</span>
<span class="text-purple-400">assert!</span>(request_scope.contains::&lt;AppConfig&gt;());
<span class="text-purple-400">assert!</span>(request_scope.contains::&lt;RequestContext&gt;());

<span class="text-slate-500">// Root cannot access request-scoped services</span>
<span class="text-purple-400">assert!</span>(!root.contains::&lt;RequestContext&gt;());`,

  // Overrides
  overrides: `<span class="text-purple-400">let</span> root = Container::new();
root.singleton(Database { url: <span class="text-green-400">"production"</span>.into() });

<span class="text-slate-500">// Create test scope with override</span>
<span class="text-purple-400">let</span> test_scope = root.scope();
test_scope.singleton(Database { url: <span class="text-green-400">"test"</span>.into() });

<span class="text-slate-500">// Root still has production</span>
<span class="text-purple-400">let</span> root_db = root.get::&lt;Database&gt;().unwrap();
<span class="text-purple-400">assert_eq!</span>(root_db.url, <span class="text-green-400">"production"</span>);

<span class="text-slate-500">// Test scope has test override</span>
<span class="text-purple-400">let</span> test_db = test_scope.get::&lt;Database&gt;().unwrap();
<span class="text-purple-400">assert_eq!</span>(test_db.url, <span class="text-green-400">"test"</span>);`,

  // Armature Framework Example
  armature: `<span class="text-purple-400">use</span> armature::prelude::*;
<span class="text-purple-400">use</span> dependency_injector::Container;
<span class="text-purple-400">use</span> std::sync::Arc;

<span class="text-slate-500">// Define an injectable database service</span>
<span class="text-purple-400">#[injectable]</span>
<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Database</span> {
    url: <span class="text-blue-400">String</span>,
}

<span class="text-slate-500">// Define a controller with injected dependencies</span>
<span class="text-purple-400">#[controller("/api")]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">UserController</span> {
    db: Arc&lt;Database&gt;,
}

<span class="text-purple-400">#[controller]</span>
<span class="text-purple-400">impl</span> UserController {
    <span class="text-purple-400">#[get("/health")]</span>
    <span class="text-purple-400">async fn</span> <span class="text-blue-400">health</span>(&<span class="text-purple-400">self</span>) -&gt; <span class="text-blue-400">Json</span>&lt;&amp;<span class="text-purple-400">str</span>&gt; {
        Json(<span class="text-green-400">"healthy"</span>)
    }

    <span class="text-purple-400">#[get("/users")]</span>
    <span class="text-purple-400">async fn</span> <span class="text-blue-400">get_users</span>(&<span class="text-purple-400">self</span>) -&gt; <span class="text-blue-400">Result</span>&lt;Json&lt;Vec&lt;User&gt;&gt;, Error&gt; {
        <span class="text-slate-500">// Access injected database</span>
        <span class="text-purple-400">let</span> users = <span class="text-purple-400">self</span>.db.query_users().<span class="text-purple-400">await</span>?;
        Ok(Json(users))
    }
}

<span class="text-slate-500">// Define the application module</span>
<span class="text-purple-400">#[module]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">AppModule</span> {
    <span class="text-purple-400">#[controllers]</span>
    controllers: (UserController,),
    <span class="text-purple-400">#[providers]</span>
    providers: (Database,),
}

<span class="text-purple-400">#[tokio::main]</span>
<span class="text-purple-400">async fn</span> <span class="text-blue-400">main</span>() -&gt; <span class="text-blue-400">Result</span>&lt;(), Error&gt; {
    <span class="text-slate-500">// Bootstrap application with DI container</span>
    Application::create(AppModule)
        .listen(<span class="text-green-400">"0.0.0.0:3000"</span>)
        .<span class="text-purple-400">await</span>
}`,

  // Testing Example
  testing: `<span class="text-purple-400">use</span> dependency_injector::Container;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">EmailService</span> {
    smtp_host: <span class="text-blue-400">String</span>,
}

<span class="text-purple-400">impl</span> EmailService {
    <span class="text-purple-400">fn</span> <span class="text-blue-400">send</span>(&<span class="text-purple-400">self</span>, to: &<span class="text-blue-400">str</span>, body: &<span class="text-blue-400">str</span>) -&gt; <span class="text-purple-400">bool</span> {
        <span class="text-slate-500">// Real implementation</span>
        <span class="text-purple-400">true</span>
    }
}

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">MockEmailService</span>;

<span class="text-purple-400">impl</span> MockEmailService {
    <span class="text-purple-400">fn</span> <span class="text-blue-400">send</span>(&<span class="text-purple-400">self</span>, _to: &<span class="text-blue-400">str</span>, _body: &<span class="text-blue-400">str</span>) -&gt; <span class="text-purple-400">bool</span> {
        <span class="text-slate-500">// Mock - always succeeds</span>
        <span class="text-purple-400">true</span>
    }
}

<span class="text-purple-400">#[test]</span>
<span class="text-purple-400">fn</span> <span class="text-blue-400">test_with_mock</span>() {
    <span class="text-purple-400">let</span> container = Container::new();

    <span class="text-slate-500">// Use mock in tests</span>
    container.singleton(MockEmailService);

    <span class="text-purple-400">let</span> email = container.get::&lt;MockEmailService&gt;().unwrap();
    <span class="text-purple-400">assert!</span>(email.send(<span class="text-green-400">"test@example.com"</span>, <span class="text-green-400">"Hello"</span>));
}`,

  // Multi-tenant Example
  multiTenant: `<span class="text-purple-400">use</span> dependency_injector::Container;
<span class="text-purple-400">use</span> std::sync::Arc;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">TenantConfig</span> {
    tenant_id: <span class="text-blue-400">String</span>,
    db_url: <span class="text-blue-400">String</span>,
}

<span class="text-purple-400">fn</span> <span class="text-blue-400">create_tenant_scope</span>(
    root: &Container,
    tenant_id: &<span class="text-blue-400">str</span>
) -&gt; Arc&lt;Container&gt; {
    <span class="text-purple-400">let</span> scope = root.scope();

    scope.singleton(TenantConfig {
        tenant_id: tenant_id.into(),
        db_url: format!(<span class="text-green-400">"postgres://localhost/{}"</span>, tenant_id),
    });

    Arc::new(scope)
}

<span class="text-purple-400">fn</span> <span class="text-blue-400">main</span>() {
    <span class="text-purple-400">let</span> root = Container::new();

    <span class="text-slate-500">// Register shared services</span>
    root.singleton(Logger::new());

    <span class="text-slate-500">// Create tenant-specific scopes</span>
    <span class="text-purple-400">let</span> tenant_a = create_tenant_scope(&root, <span class="text-green-400">"tenant_a"</span>);
    <span class="text-purple-400">let</span> tenant_b = create_tenant_scope(&root, <span class="text-green-400">"tenant_b"</span>);

    <span class="text-slate-500">// Each tenant has isolated config</span>
    <span class="text-purple-400">let</span> config_a = tenant_a.get::&lt;TenantConfig&gt;().unwrap();
    <span class="text-purple-400">let</span> config_b = tenant_b.get::&lt;TenantConfig&gt;().unwrap();

    <span class="text-purple-400">assert_ne!</span>(config_a.tenant_id, config_b.tenant_id);
}`,
  // FFI - Go
  ffiGo: `<span class="text-purple-400">package</span> main

<span class="text-purple-400">import</span> (
    <span class="text-green-400">"fmt"</span>
    <span class="text-green-400">"github.com/pegasusheavy/dependency-injector/ffi/go/di"</span>
)

<span class="text-purple-400">type</span> <span class="text-blue-400">Config</span> <span class="text-purple-400">struct</span> {
    Debug <span class="text-blue-400">bool</span>   \`json:<span class="text-green-400">"debug"</span>\`
    Port  <span class="text-blue-400">int</span>    \`json:<span class="text-green-400">"port"</span>\`
}

<span class="text-purple-400">func</span> <span class="text-blue-400">main</span>() {
    <span class="text-slate-500">// Create container</span>
    container := di.NewContainer()
    <span class="text-purple-400">defer</span> container.Free()

    <span class="text-slate-500">// Register service</span>
    container.RegisterValue(<span class="text-green-400">"Config"</span>, Config{Debug: <span class="text-purple-400">true</span>, Port: 8080})

    <span class="text-slate-500">// Resolve service</span>
    <span class="text-purple-400">var</span> config Config
    container.ResolveJSON(<span class="text-green-400">"Config"</span>, &config)

    fmt.Printf(<span class="text-green-400">"Port: %d\\n"</span>, config.Port)
}`,

  // FFI - Node.js
  ffiNodejs: `<span class="text-purple-400">import</span> { Container } <span class="text-purple-400">from</span> <span class="text-green-400">'@pegasusheavy/dependency-injector'</span>;

<span class="text-slate-500">// Define types</span>
<span class="text-purple-400">interface</span> <span class="text-blue-400">Config</span> {
  debug: <span class="text-blue-400">boolean</span>;
  port: <span class="text-blue-400">number</span>;
}

<span class="text-slate-500">// Create container</span>
<span class="text-purple-400">const</span> container = <span class="text-purple-400">new</span> Container();

<span class="text-purple-400">try</span> {
  <span class="text-slate-500">// Register service</span>
  container.register&lt;Config&gt;(<span class="text-green-400">'Config'</span>, { debug: <span class="text-purple-400">true</span>, port: 8080 });

  <span class="text-slate-500">// Resolve service</span>
  <span class="text-purple-400">const</span> config = container.resolve&lt;Config&gt;(<span class="text-green-400">'Config'</span>);
  console.log(<span class="text-green-400">\`Port: \${config.port}\`</span>);

  <span class="text-slate-500">// Optional resolution</span>
  <span class="text-purple-400">const</span> missing = container.tryResolve&lt;Config&gt;(<span class="text-green-400">'Missing'</span>);
  <span class="text-slate-500">// missing is null</span>
} <span class="text-purple-400">finally</span> {
  container.free();
}`,

  // FFI - Python
  ffiPython: `<span class="text-purple-400">from</span> dependency_injector <span class="text-purple-400">import</span> Container

<span class="text-slate-500"># Create container (with context manager)</span>
<span class="text-purple-400">with</span> Container() <span class="text-purple-400">as</span> container:
    <span class="text-slate-500"># Register service</span>
    container.register(<span class="text-green-400">"Config"</span>, {
        <span class="text-green-400">"debug"</span>: <span class="text-purple-400">True</span>,
        <span class="text-green-400">"port"</span>: 8080
    })

    <span class="text-slate-500"># Resolve service</span>
    config = container.resolve(<span class="text-green-400">"Config"</span>)
    <span class="text-purple-400">print</span>(<span class="text-green-400">f"Port: {config['port']}"</span>)

    <span class="text-slate-500"># Optional resolution</span>
    missing = container.try_resolve(<span class="text-green-400">"Missing"</span>)
    <span class="text-slate-500"># missing is None</span>

    <span class="text-slate-500"># Check existence</span>
    <span class="text-purple-400">print</span>(container.contains(<span class="text-green-400">"Config"</span>))  <span class="text-slate-500"># True</span>

<span class="text-slate-500"># Container auto-freed after 'with' block</span>`,

  // FFI - C#
  ffiCsharp: `<span class="text-purple-400">using</span> DependencyInjector;

<span class="text-slate-500">// Define types</span>
<span class="text-purple-400">record</span> <span class="text-blue-400">Config</span>(<span class="text-blue-400">bool</span> Debug, <span class="text-blue-400">int</span> Port);

<span class="text-slate-500">// Create container (with using statement)</span>
<span class="text-purple-400">using</span> <span class="text-purple-400">var</span> container = <span class="text-purple-400">new</span> Container();

<span class="text-slate-500">// Register service</span>
container.Register(<span class="text-green-400">"Config"</span>, <span class="text-purple-400">new</span> Config(Debug: <span class="text-purple-400">true</span>, Port: 8080));

<span class="text-slate-500">// Resolve service</span>
<span class="text-purple-400">var</span> config = container.Resolve&lt;Config&gt;(<span class="text-green-400">"Config"</span>);
Console.WriteLine(<span class="text-green-400">$"Port: {config.Port}"</span>);

<span class="text-slate-500">// Optional resolution</span>
<span class="text-purple-400">var</span> missing = container.TryResolve&lt;Config&gt;(<span class="text-green-400">"Missing"</span>);
<span class="text-slate-500">// missing is null</span>

<span class="text-slate-500">// Check existence</span>
Console.WriteLine(container.Contains(<span class="text-green-400">"Config"</span>));  <span class="text-slate-500">// True</span>`,

  // FFI - Build Instructions
  ffiBuild: `<span class="text-slate-500"># Build the FFI shared library</span>
cargo rustc --release --features ffi --crate-type cdylib

<span class="text-slate-500"># Output locations:</span>
<span class="text-slate-500"># Linux:   target/release/libdependency_injector.so</span>
<span class="text-slate-500"># macOS:   target/release/libdependency_injector.dylib</span>
<span class="text-slate-500"># Windows: target/release/dependency_injector.dll</span>

<span class="text-slate-500"># Set library path (Linux)</span>
export LD_LIBRARY_PATH=/path/to/target/release:$LD_LIBRARY_PATH

<span class="text-slate-500"># Set library path (macOS)</span>
export DYLD_LIBRARY_PATH=/path/to/target/release:$DYLD_LIBRARY_PATH`,

  // FFI - Scoped containers example
  ffiScopes: `<span class="text-slate-500">// Go example - scoped containers</span>
root := di.NewContainer()
<span class="text-purple-400">defer</span> root.Free()

root.RegisterValue(<span class="text-green-400">"Config"</span>, Config{Env: <span class="text-green-400">"production"</span>})

<span class="text-slate-500">// Create child scope</span>
request, _ := root.Scope()
<span class="text-purple-400">defer</span> request.Free()

request.RegisterValue(<span class="text-green-400">"RequestId"</span>, RequestId{Id: <span class="text-green-400">"req-123"</span>})

<span class="text-slate-500">// Child can access parent</span>
<span class="text-purple-400">var</span> config Config
request.ResolveJSON(<span class="text-green-400">"Config"</span>, &config) <span class="text-slate-500">// Works!</span>

<span class="text-slate-500">// Parent cannot access child</span>
root.Contains(<span class="text-green-400">"RequestId"</span>) <span class="text-slate-500">// false</span>`,
} as const;

export type CodeSnippetKey = keyof typeof CODE_SNIPPETS;

