# Plan: lablog v0.1.0 — bugs críticos y producción

> Dejar lablog listo para release público `v0.1.0`: corregir los bugs que rompen la
> experiencia (voz, preview, celdas), cerrar agujeros de calidad, publicar en PyPI y
> garantizar que un usuario nuevo pueda instalar y usar la app sin errores.

## Bugs encontrados (raíz)

| Bug | Archivo(s) | Raíz |
|---|---|---|
| **Dictado de voz congela / bucle infinito** | `ui/src/components/shell/toolbar.tsx`, `ui/src/hooks/use-speech.ts` | El efecto de `toolbar` depende de `activeLatex`; tras insertar el dictado se vuelve a disparar porque `transcript` nunca se limpia. |
| **Celdas sin opciones se ignoran si hay celdas con opciones** | `ui/src/lib/latex-parser.ts`, `src/lablog/latex_ast.py` | El parser solo busca celdas sin `[...]` cuando **no** encuentra ninguna celda con opciones. |
| **Recargas infinitas al iniciar** | `ui/src/components/shell/app-shell.tsx`, `ui/src/App.tsx` | `useAppStore()` sin selector devuelve todo el estado; el efecto de carga vuelve a ejecutarse en cada render. |
| **Celdas Python inestables / errores opacos** | `src/lablog/code_engine.py`, `src/lablog/api.py`, `ui/src/components/panels/cells-panel.tsx`, `ui/src/components/lab/lab-canvas.tsx` | Singleton de kernel sin lock, arranque perezoso sin manejo de errores, UI oculta el mensaje real del backend y permite ejecutar lenguajes no soportados. |
| **Preview aproximado y reconstrucción forzada** | `ui/src/lib/latex-render.ts`, `ui/src/components/preview/latex-preview.tsx` | Se reconstruye LaTeX desde el AST para volver a parsearlo, se pierde formato original y no hay feedback de errores de KaTeX. |
| **XSS en previsualización Markdown del lab** | `ui/src/components/lab/lab-canvas.tsx` | `renderInlineLatex` no escapa HTML antes de inyectarlo. |

## Tasks

- [ ] **1. Arreglar dictado de voz** (`toolbar.tsx` + `use-speech.ts`)
  - Añadir `resetTranscript` al hook y llamarlo tras insertar el dictado.
  - Romper la dependencia circular del efecto (`activeLatex` no debe repetir el append).
  - **Verificar:** test unitario que simula `stop` + `transcript`; manualmente dictar no congela la UI.

- [ ] **2. Corregir parser de celdas** (`latex-parser.ts` + `latex_ast.py`)
  - Unificar regex con/sin opciones en un solo paso y asignar IDs después de ordenar.
  - **Verificar:** test con `\begin{python}...\end{python}` junto a `\begin{python}[label=x]...\end{python}` genera dos nodos.

- [ ] **3. Refactorizar consumo de Zustand**
  - Usar selectores por campo en componentes críticos (`app-shell`, `App`, `toolbar`, etc.) para evitar re-renderizados y bucles de efectos.
  - **Verificar:** React DevTools Profiler muestra menos renders; `listPages` se llama solo una vez al montar.

- [ ] **4. Hacer el CodeEngine robusto** (`code_engine.py` + `api.py`)
  - Añadir lock de ejecución, capturar fallos de arranque del kernel, retornar 503/JSON con error legible, reiniciar kernel si muere.
  - **Verificar:** tests de ejecución concurrente y de kernel no disponible; endpoint devuelve mensaje útil.

- [ ] **5. Mejorar UX de ejecución de celdas** (`cells-panel.tsx` + `lab-canvas.tsx`)
  - Mostrar el error real del backend; deshabilitar "Ejecutar" para lenguajes no soportados (markdown/latex).
  - **Verificar:** test de UI que asserta el mensaje de error tras un 500/422 simulado.

- [ ] **6. Mejorar preview aproximado** (`latex-render.ts` + `latex-preview.tsx`)
  - Renderizar directamente desde AST sin reconstruir LaTeX, mostrar bloques de error de KaTeX, detectar documento completo de forma más fiable.
  - **Verificar:** tests de `renderDocument` con AST complejo; fórmulas display/inline se renderizan correctamente.

- [ ] **7. Sanear HTML en MarkdownPreview del lab** (`lab-canvas.tsx`)
  - Escapar contenido antes de renderizar math inline.
  - **Verificar:** test con `<script>alert(1)</script>` no ejecuta código.

- [ ] **8. Subir cobertura y calidad de tests**
  - Backend ≥80%, frontend añadir tests de voz/parser/render/celdas; ajustar pre-commit para incluir backend tests y no correr frontend tests en cada commit.
  - **Verificar:** `uv run pytest` y `npm test` pasan; coverage sube.

- [ ] **9. Preparar release v0.1.0**
  - Añadir workflow de GitHub Actions para release + PyPI, crear `CHANGELOG.md`, actualizar badges del README, empaquetar `lablog desktop` smoke test.
  - **Verificar:** `uv build` genera wheel/sdist; `twine check` pasa; workflow hace release al crear tag `v0.1.0`.

- [ ] **10. Añadir Error Boundaries y degradación graceful**
  - Envolver preview y lab canvas para no tumbar la app; backend devolver 503 cuando el kernel no esté listo.
  - **Verificar:** test de error boundary renderiza fallback; `/health` indica disponibilidad de kernel.

## Done When

- [ ] Los 3 bugs reportados (voz, preview, celdas) están corregidos y tienen tests de regresión.
- [ ] CI es verde (`backend` + `frontend`) y pre-commit no es excesivamente lento.
- [ ] `lablog` se instala desde PyPI (o al menos el wheel local) y `lablog serve` + UI funciona.
- [ ] README y `docs/IMPLEMENTATION_PLAN.md` reflejan el estado real y el roadmap de producción.

## Notas / Riesgos

- Los cambios de Zustand tocan muchos archivos; se harán en un commit aparte para facilitar la revisión.
- El CodeEngine con Jupyter no es thread-safe por diseño; el lock limita ejecución concurrente a una celda a la vez.
- PyPI requiere cuenta/API token configurada en GitHub Secrets (`PYPI_API_TOKEN`).
