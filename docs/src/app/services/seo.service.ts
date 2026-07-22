import { Injectable, inject } from '@angular/core';
import { Meta, Title } from '@angular/platform-browser';
import { DOCUMENT } from '@angular/common';

export interface SeoConfig {
  title: string;
  description: string;
  keywords?: string[];
  canonical?: string;
  ogImage?: string;
  ogType?: 'website' | 'article';
  twitterCard?: 'summary' | 'summary_large_image';
  jsonLd?: object;
}

@Injectable({
  providedIn: 'root'
})
export class SeoService {
  private readonly meta = inject(Meta);
  private readonly title = inject(Title);
  private readonly document = inject(DOCUMENT);

  private readonly baseUrl = 'https://pegasusheavy.github.io/dependency-injector';
  private readonly defaultImage = `${this.baseUrl}/og-image.png`;
  private readonly siteName = 'Dependency Injector';

  private readonly defaultKeywords = [
    'rust dependency injection',
    'di container',
    'ioc container',
    'go dependency injection',
    'nodejs di',
    'python di',
    'csharp di',
    'ffi bindings',
    'lock-free',
    'thread-safe',
    'high-performance'
  ];

  updateSeo(config: SeoConfig): void {
    // Update title
    this.title.setTitle(config.title);

    // Update meta tags
    this.updateMetaTag('description', config.description);
    this.updateMetaTag('keywords', [...(config.keywords || []), ...this.defaultKeywords].join(', '));

    // Open Graph
    this.updateMetaTag('og:title', config.title, 'property');
    this.updateMetaTag('og:description', config.description, 'property');
    this.updateMetaTag('og:type', config.ogType || 'website', 'property');
    this.updateMetaTag('og:image', config.ogImage || this.defaultImage, 'property');
    this.updateMetaTag('og:site_name', this.siteName, 'property');
    this.updateMetaTag('og:url', config.canonical || this.baseUrl, 'property');

    // Twitter
    this.updateMetaTag('twitter:card', config.twitterCard || 'summary_large_image');
    this.updateMetaTag('twitter:title', config.title);
    this.updateMetaTag('twitter:description', config.description);
    this.updateMetaTag('twitter:image', config.ogImage || this.defaultImage);

    // Canonical URL
    if (config.canonical) {
      this.updateCanonical(config.canonical);
    }

    // JSON-LD
    if (config.jsonLd) {
      this.updateJsonLd(config.jsonLd);
    }
  }

  private updateMetaTag(name: string, content: string, attr: 'name' | 'property' = 'name'): void {
    const selector = attr === 'property' ? `property="${name}"` : `name="${name}"`;

    if (this.meta.getTag(selector)) {
      this.meta.updateTag({ [attr]: name, content });
    } else {
      this.meta.addTag({ [attr]: name, content });
    }
  }

  private updateCanonical(url: string): void {
    let link = this.document.querySelector('link[rel="canonical"]') as HTMLLinkElement;

    if (!link) {
      link = this.document.createElement('link');
      link.setAttribute('rel', 'canonical');
      this.document.head.appendChild(link);
    }

    link.setAttribute('href', url);
  }

  private updateJsonLd(data: object): void {
    // Remove existing dynamic JSON-LD
    const existingScript = this.document.getElementById('dynamic-jsonld');
    if (existingScript) {
      existingScript.remove();
    }

    // Add new JSON-LD
    const script = this.document.createElement('script');
    script.id = 'dynamic-jsonld';
    script.type = 'application/ld+json';
    script.textContent = JSON.stringify(data);
    this.document.head.appendChild(script);
  }

  // Pre-configured SEO for common pages
  setHomeSeo(): void {
    this.updateSeo({
      title: 'Dependency Injector - High-Performance DI for Rust, Go, Node.js, Python & C#',
      description: 'Lightning-fast dependency injection with sub-20ns cached resolution. Cross-language FFI bindings for Go, Node.js, Python, and C#. Lock-free, thread-safe, type-safe.',
      keywords: ['rust library', 'cross-language di', 'performance'],
      canonical: this.baseUrl,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'WebPage',
        name: 'Dependency Injector',
        description: 'High-performance dependency injection for Rust with cross-language support',
        url: this.baseUrl
      }
    });
  }

  setBenchmarksSeo(): void {
    this.updateSeo({
      title: 'Benchmarks - Dependency Injector Performance Comparison',
      description: 'Comprehensive benchmarks comparing dependency-injector against other DI solutions in Rust, Go, Node.js, Python, and C#. See how we achieve 17-32ns singleton resolution.',
      keywords: ['benchmark', 'performance comparison', 'di performance', 'rust benchmark'],
      canonical: `${this.baseUrl}/benchmarks`,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'WebPage',
        name: 'Dependency Injector Benchmarks',
        description: 'Performance benchmarks and comparisons',
        url: `${this.baseUrl}/benchmarks`
      }
    });
  }

  setGettingStartedSeo(): void {
    this.updateSeo({
      title: 'Getting Started - Dependency Injector',
      description: 'Quick start guide for dependency-injector. Learn how to install and use high-performance dependency injection in Rust, Go, Node.js, Python, and C# in minutes.',
      keywords: ['tutorial', 'quick start', 'installation', 'getting started'],
      canonical: `${this.baseUrl}/docs/getting-started`,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'HowTo',
        name: 'Getting Started with Dependency Injector',
        description: 'Step-by-step guide to using dependency-injector',
        url: `${this.baseUrl}/docs/getting-started`
      }
    });
  }

  setGuideSeo(): void {
    this.updateSeo({
      title: 'Guide - Dependency Injector',
      description: 'Comprehensive guide to dependency-injector features: singletons, transients, scoped containers, factories, and more. Learn advanced patterns for Rust DI.',
      keywords: ['guide', 'tutorial', 'singleton', 'transient', 'scoped', 'factory'],
      canonical: `${this.baseUrl}/docs/guide`,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'TechArticle',
        name: 'Dependency Injector Guide',
        description: 'Comprehensive guide to dependency injection patterns',
        url: `${this.baseUrl}/docs/guide`
      }
    });
  }

  setApiSeo(): void {
    this.updateSeo({
      title: 'API Reference - Dependency Injector',
      description: 'Complete API reference for dependency-injector. Documentation for Container, Factory, Scope, and all public types and methods.',
      keywords: ['api', 'reference', 'documentation', 'container', 'factory', 'scope'],
      canonical: `${this.baseUrl}/docs/api`,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'TechArticle',
        name: 'Dependency Injector API Reference',
        description: 'Complete API documentation',
        url: `${this.baseUrl}/docs/api`
      }
    });
  }

  setExamplesSeo(): void {
    this.updateSeo({
      title: 'Examples - Dependency Injector',
      description: 'Code examples for dependency-injector in Rust, Go, Node.js, Python, and C#. From basic usage to advanced patterns like scoped containers and factory providers.',
      keywords: ['examples', 'code samples', 'rust examples', 'go examples', 'nodejs examples', 'python examples', 'csharp examples'],
      canonical: `${this.baseUrl}/docs/examples`,
      jsonLd: {
        '@context': 'https://schema.org',
        '@type': 'CollectionPage',
        name: 'Dependency Injector Examples',
        description: 'Code examples for multiple languages',
        url: `${this.baseUrl}/docs/examples`
      }
    });
  }
}



