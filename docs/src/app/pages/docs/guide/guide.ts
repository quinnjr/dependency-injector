import { Component, OnInit, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { FontAwesomeModule } from '@fortawesome/angular-fontawesome';
import { CodeBlockComponent } from '../../../components/code-block/code-block';
import { CODE_SNIPPETS } from '../../../data/code-snippets';
import { SeoService } from '../../../services/seo.service';

@Component({
  selector: 'app-guide',
  imports: [RouterLink, FontAwesomeModule, CodeBlockComponent],
  templateUrl: './guide.html',
  styleUrl: './guide.scss'
})
export class GuidePage implements OnInit {
  private readonly seo = inject(SeoService);

  scopeCode = CODE_SNIPPETS.scopes;
  overrideCode = CODE_SNIPPETS.overrides;

  typedBuilderCode = `<span class="text-purple-400">use</span> dependency_injector::typed::TypedBuilder;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Database</span> { url: <span class="text-blue-400">String</span> }

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">Cache</span> { size: <span class="text-blue-400">usize</span> }

<span class="text-slate-500">// The builder's type parameter records every registration</span>
<span class="text-purple-400">let</span> container = TypedBuilder::new()
    .singleton(Database { url: <span class="text-green-400">"postgres://localhost"</span>.into() })
    .lazy(|| Cache { size: <span class="text-yellow-400">1024</span> })
    .build();  <span class="text-slate-500">// locks the container</span>

<span class="text-slate-500">// Resolution returns Arc&lt;T&gt; directly - no Result to unwrap</span>
<span class="text-purple-400">let</span> db = container.get::&lt;Database&gt;();
<span class="text-purple-400">let</span> cache = container.get::&lt;Cache&gt;();`;

  verifiedServiceCode = `<span class="text-purple-400">use</span> dependency_injector::{Container, verified::{Service, ServiceProvider}};
<span class="text-purple-400">use</span> std::sync::Arc;

<span class="text-purple-400">#[derive(Clone)]</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">UserRepository</span> {
    db: Arc&lt;Database&gt;,
    cache: Option&lt;Arc&lt;Cache&gt;&gt;,  <span class="text-slate-500">// optional dependency</span>
}

<span class="text-purple-400">impl</span> Service <span class="text-purple-400">for</span> <span class="text-blue-400">Database</span> {
    <span class="text-purple-400">type</span> Dependencies = ();
    <span class="text-purple-400">fn</span> <span class="text-blue-400">create</span>(_: ()) -&gt; <span class="text-blue-400">Self</span> {
        Database { url: <span class="text-green-400">"postgres://localhost"</span>.into() }
    }
}

<span class="text-purple-400">impl</span> Service <span class="text-purple-400">for</span> <span class="text-blue-400">UserRepository</span> {
    <span class="text-slate-500">// Tuples (up to 12 elements) mix required and optional deps</span>
    <span class="text-purple-400">type</span> Dependencies = (Arc&lt;Database&gt;, Option&lt;Arc&lt;Cache&gt;&gt;);
    <span class="text-purple-400">fn</span> <span class="text-blue-400">create</span>((db, cache): <span class="text-blue-400">Self</span>::Dependencies) -&gt; <span class="text-blue-400">Self</span> {
        UserRepository { db, cache }
    }
}

<span class="text-purple-400">let</span> container = Container::new();
container.provide::&lt;Database&gt;();  <span class="text-slate-500">// lazy: created on first resolve</span>

<span class="text-slate-500">// Eager: deps resolved now; returns false if a required dep is missing</span>
<span class="text-purple-400">assert!</span>(container.provide_singleton::&lt;UserRepository&gt;());

<span class="text-purple-400">let</span> repo = container.get::&lt;UserRepository&gt;().unwrap();
<span class="text-purple-400">assert!</span>(repo.cache.is_none());  <span class="text-slate-500">// Cache never registered - optional</span>`;

  serviceModuleCode = `<span class="text-purple-400">use</span> dependency_injector::{Container, verified::{ServiceModule, ServiceProvider}};

<span class="text-slate-500">// Database and Cache implement Service as shown above</span>
<span class="text-purple-400">struct</span> <span class="text-blue-400">DataModule</span>;

<span class="text-purple-400">impl</span> ServiceModule <span class="text-purple-400">for</span> <span class="text-blue-400">DataModule</span> {
    <span class="text-purple-400">fn</span> <span class="text-blue-400">register</span>(container: &amp;Container) {
        container.provide::&lt;Database&gt;();
        container.provide::&lt;Cache&gt;();
    }
}

<span class="text-purple-400">let</span> container = Container::new();
DataModule::register(&amp;container);

<span class="text-purple-400">assert!</span>(container.contains::&lt;Database&gt;());
<span class="text-purple-400">assert!</span>(container.contains::&lt;Cache&gt;());`;

  typedRequireCode = `<span class="text-purple-400">use</span> dependency_injector::typed::{Reg, Require, TypedBuilder, TypedContainer};

<span class="text-slate-500">// Hand-written: declare dependencies as a type-level Reg list</span>
<span class="text-purple-400">impl</span> Require <span class="text-purple-400">for</span> <span class="text-blue-400">UserService</span> {
    <span class="text-purple-400">type</span> Dependencies = Reg&lt;Database, Reg&lt;Cache, ()&gt;&gt;;
}

<span class="text-slate-500">// Or derive it (requires the \`derive\` feature):</span>
<span class="text-slate-500">// #[derive(Clone, TypedRequire)]</span>
<span class="text-slate-500">// struct UserService {</span>
<span class="text-slate-500">//     #[dep]</span>
<span class="text-slate-500">//     db: Arc&lt;Database&gt;,</span>
<span class="text-slate-500">//     #[dep(optional)]  // excluded from the required list</span>
<span class="text-slate-500">//     cache: Option&lt;Arc&lt;Cache&gt;&gt;,</span>
<span class="text-slate-500">// }</span>

<span class="text-slate-500">// Compile-time check: the registry type must match the declared deps</span>
<span class="text-purple-400">fn</span> <span class="text-blue-400">assert_deps</span>&lt;S: Require&lt;Dependencies = R&gt;, R&gt;(_: &amp;TypedContainer&lt;R&gt;) {}

<span class="text-slate-500">// Registration order builds the registry head-first:</span>
<span class="text-slate-500">// Cache, then Database =&gt; Reg&lt;Database, Reg&lt;Cache, ()&gt;&gt;</span>
<span class="text-purple-400">let</span> container = TypedBuilder::new()
    .singleton(Cache)
    .singleton(Database)
    .build();

<span class="text-slate-500">// Fails to compile if a dependency is missing or misordered</span>
assert_deps::&lt;UserService, _&gt;(&amp;container);`;

  ngOnInit(): void {
    this.seo.setGuideSeo();
  }
}
