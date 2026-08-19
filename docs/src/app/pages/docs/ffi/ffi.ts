import { Component, OnInit, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { FontAwesomeModule } from '@fortawesome/angular-fontawesome';
import { CodeBlockComponent } from '../../../components/code-block/code-block';
import { CODE_SNIPPETS } from '../../../data/code-snippets';
import { SeoService } from '../../../services/seo.service';

@Component({
  selector: 'app-ffi',
  imports: [RouterLink, FontAwesomeModule, CodeBlockComponent],
  templateUrl: './ffi.html',
  styleUrl: './ffi.scss'
})
export class FfiPage implements OnInit {
  private readonly seo = inject(SeoService);

  buildCode = CODE_SNIPPETS.ffiBuild;
  goCode = CODE_SNIPPETS.ffiGo;
  nodejsCode = CODE_SNIPPETS.ffiNodejs;
  pythonCode = CODE_SNIPPETS.ffiPython;
  csharpCode = CODE_SNIPPETS.ffiCsharp;
  scopesCode = CODE_SNIPPETS.ffiScopes;

  languages = [
    {
      name: 'Go',
      icon: 'golang',
      description: 'High-performance Go bindings via cgo',
      install: 'go get github.com/quinnjr/dependency-injector/ffi/go/di',
      features: ['Struct serialization', 'Error handling with sentinel errors', 'Finalizer-based cleanup']
    },
    {
      name: 'Node.js',
      icon: 'node-js',
      description: 'TypeScript bindings via koffi (no native compilation)',
      install: 'pnpm add dependency-injector',
      features: ['Full TypeScript support', 'Generic type inference', 'SWC-powered builds']
    },
    {
      name: 'Python',
      icon: 'python',
      description: 'Zero-dependency bindings via ctypes',
      install: 'pip install dependency-injector-rust',
      features: ['Context manager support', 'Type hints', 'No compilation required']
    },
    {
      name: 'C#',
      icon: 'microsoft',
      description: '.NET 8.0+ bindings via P/Invoke',
      install: 'dotnet add package DependencyInjector',
      features: ['Record type support', 'IDisposable pattern', 'Generic resolution']
    }
  ];

  ngOnInit(): void {
    this.seo.updateSeo({
      title: 'FFI Bindings - Go, Node.js, Python, C# | Dependency Injector',
      description: 'Use the Rust dependency-injector from Go, Node.js/TypeScript, Python, and C#. Cross-language FFI bindings with ~300ns resolution.',
      keywords: ['ffi', 'go bindings', 'nodejs bindings', 'python bindings', 'csharp bindings', 'cross-language'],
      canonical: 'https://quinnjr.github.io/dependency-injector/docs/ffi'
    });
  }
}



