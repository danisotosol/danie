# Fixes para danie v0.1.1 — pasa esto a Agent A

Verificado por ox-alpha el 2026-08-22 (tests corridos + sesión live con OpenRouter OK).
Ordenado por prioridad. Repo: C:\Users\danie\Downloads\danie

## 1. Default de idioma del perfil: "es" → "en"
- `crates/danie-core/src/profile.rs` línea ~27: `language: "es".to_string()`.
- Producto apunta a estudiantes americanos; inglés debe ser default.
- Actualizar el test `language_line_is_optional_and_defaults_to_spanish` acorde.

## 2. Truncar cuerpos de error HTTP
- Al fallar con base_url malo, OpenRouter devuelve una página HTML entera y el TUI
  muestra un muro ilegible (visto en doctor con status 404).
- En `crates/danie-llm` (error.rs / providers): truncar body a ~200 chars,
  idealmente detectando HTML (`<html`) y resumiendo a "non-JSON response (HTML page)".

## 3. Persistir el plan entre sesiones
- Hoy retomar una sesión regenera el DAG con 1 llamada LLM (costo + inconsistencia).
- El DAG ya vive en danie-core; serializarlo al store `.danie/` y reutilizarlo si existe.

## 4. "Mark known anyway" debe preguntar calidad SM-2
- Ahora fija quality = Good silenciosamente. Pedir Again/Hard/Good/Easy al usuario.

## 5. Wrap de texto por caracteres → por graphemes
- Bajo prioridad; solo importa si se espera contenido CJK.

## Nota de contexto (no requiere acción ya)
- Config validada: provider openai-compat, modelo stealth/ox-alpha via Nous proxy
  (http://127.0.0.1:8645/v1). OpenRouter fallback documentado en README.

## 7. RESUELTO (2026-08-22, commit 0735b92): parser JSON con modelos reasoning
- `sanitize_json_bare_control_chars()` en danie-engine/protocol.rs: fallback que
  escapa \n, \r, \t y controles crudos dentro de strings JSON cuando el parse
  estricto falla. 3 tests nuevos. El perfil .danie/profile.md ahora está en EN.
