// C interface to the Rust engine (engine/src/ffi.rs). Used by the macOS shell via a bridging header.
#pragma once
#include <stddef.h>
typedef struct WzEngine WzEngine;
WzEngine *wz_engine_new(const char *lexicon_json_path, const char *state_path_or_null);
char *wz_engine_next_card_json(WzEngine *engine);
size_t wz_engine_count(const WzEngine *engine);
void wz_string_free(char *s);
void wz_engine_free(WzEngine *engine);
