import { Component, OnInit, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { FontAwesomeModule } from '@fortawesome/angular-fontawesome';
import { SeoService } from '../../../services/seo.service';

@Component({
  selector: 'app-api',
  imports: [RouterLink, FontAwesomeModule],
  templateUrl: './api.html',
  styleUrl: './api.scss'
})
export class ApiPage implements OnInit {
  private readonly seo = inject(SeoService);

  containerMethods = [
    { name: 'new()', description: 'Creates a new empty container', returns: 'Container' },
    { name: 'singleton<T>(value: T)', description: 'Registers a singleton service with an immediate value', returns: '()' },
    { name: 'lazy<T, F>(factory: F)', description: 'Registers a lazy singleton that is created on first access', returns: '()' },
    { name: 'transient<T, F>(factory: F)', description: 'Registers a transient service with a factory', returns: '()' },
    { name: 'get<T>()', description: 'Resolves a service, returning an Arc<T>', returns: 'Result<Arc<T>, DiError>' },
    { name: 'try_get<T>()', description: 'Tries to resolve a service, returning None if not found', returns: 'Option<Arc<T>>' },
    { name: 'contains<T>()', description: 'Checks if a service is registered', returns: 'bool' },
    { name: 'scope()', description: 'Creates a child scope that inherits from this container', returns: 'Container' },
    { name: 'lock()', description: 'Prevents further registrations', returns: '()' },
  ];

  errorVariants = [
    { name: 'NotFound', description: 'The requested service type was not found in the container or any parent scope' },
    { name: 'CircularDependency', description: 'A circular dependency was detected while resolving the service' },
    { name: 'CreationFailed', description: 'The factory failed to create the service, with a reason describing the failure' },
    { name: 'Locked', description: 'The container is locked and cannot register new services' },
    { name: 'AlreadyRegistered', description: 'Attempted to register a service type that is already registered' },
    { name: 'ParentDropped', description: 'The parent scope has been dropped' },
    { name: 'Internal', description: 'An internal DI error occurred' },
  ];

  ngOnInit(): void {
    this.seo.setApiSeo();
  }
}
