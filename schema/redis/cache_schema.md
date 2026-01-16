# Redis Cache Schema - Drone Convoy Tracking System
# =============================================================================
# Classification: UNCLASSIFIED // FOR OFFICIAL USE ONLY
# Version: 1.0.0
# =============================================================================

## Design Principles

1. **Cache-Aside Pattern**: Check Redis first, fallback to ScyllaDB on miss
2. **Write-Through for Hot Data**: Leaderboard updates written to both
3. **TTL Strategy**: Tiered based on data volatility
4. **Key Namespacing**: Hierarchical keys for organization and bulk invalidation

---

## Key Namespace Convention

```
{domain}:{entity}:{identifier}:{attribute?}
```

Examples:
- `convoy:leaderboard:550e8400-e29b-41d4-a716-446655440000`
- `drone:position:660e8400-e29b-41d4-a716-446655440001`
- `telemetry:latest:660e8400-e29b-41d4-a716-446655440001`

---

## Cache Structures

### 1. Leaderboard Cache (ZSET)
**Purpose**: Real-time sorted leaderboard by accuracy percentage

```redis
KEY:    convoy:leaderboard:{convoy_id}
TYPE:   Sorted Set (ZSET)
SCORE:  accuracy_pct (0.0 - 100.0)
MEMBER: drone_id (UUID string)
TTL:    300 seconds (5 minutes)

# Commands:
ZADD convoy:leaderboard:{convoy_id} {accuracy_pct} {drone_id}
ZREVRANGE convoy:leaderboard:{convoy_id} 0 9 WITHSCORES  # Top 10
ZREVRANK convoy:leaderboard:{convoy_id} {drone_id}       # Get rank
ZINCRBY convoy:leaderboard:{convoy_id} {delta} {drone_id} # Update score
```

### 2. Drone State Cache (HASH)
**Purpose**: Current drone state for fast lookups

```redis
KEY:    drone:state:{drone_id}
TYPE:   Hash
TTL:    60 seconds (frequently updated)

FIELDS:
  convoy_id         UUID
  callsign          string
  status            string (AIRBORNE, LOITER, etc.)
  platform_type     string
  lat               float
  lon               float
  altitude_m        float
  heading_deg       float
  speed_mps         float
  fuel_pct          float
  accuracy_pct      float
  total_engagements int
  successful_hits   int
  current_waypoint  int
  updated_at        ISO8601 timestamp

# Commands:
HGETALL drone:state:{drone_id}
HMSET drone:state:{drone_id} lat {lat} lon {lon} ...
HINCRBY drone:state:{drone_id} total_engagements 1
EXPIRE drone:state:{drone_id} 60
```

### 3. Convoy Roster Cache (SET)
**Purpose**: Quick lookup of all drones in a convoy

```redis
KEY:    convoy:roster:{convoy_id}
TYPE:   Set
TTL:    3600 seconds (1 hour - convoy membership rarely changes)

# Commands:
SADD convoy:roster:{convoy_id} {drone_id}
SMEMBERS convoy:roster:{convoy_id}
SISMEMBER convoy:roster:{convoy_id} {drone_id}
SCARD convoy:roster:{convoy_id}  # Count
```

### 4. Latest Telemetry Cache (STRING/JSON)
**Purpose**: Most recent telemetry snapshot per drone

```redis
KEY:    telemetry:latest:{drone_id}
TYPE:   String (JSON serialized)
TTL:    10 seconds (very hot, frequently updated)

VALUE:  {
  "drone_id": "uuid",
  "recorded_at": "ISO8601",
  "position": {
    "latitude": 34.5553,
    "longitude": 69.2075,
    "altitude_m": 5000.0,
    "heading_deg": 45.0,
    "speed_mps": 80.0
  },
  "fuel_remaining_pct": 75.5,
  "current_waypoint": 12,
  "distance_to_next_km": 15.3,
  "mesh_connectivity": 0.95
}

# Commands:
SET telemetry:latest:{drone_id} '{json}' EX 10
GET telemetry:latest:{drone_id}
```

### 5. Engagement Stats Cache (HASH)
**Purpose**: Running engagement statistics for quick accuracy calculation

```redis
KEY:    stats:engagements:{drone_id}
TYPE:   Hash
TTL:    300 seconds (5 minutes)

FIELDS:
  total_engagements  int
  successful_hits    int
  current_streak     int
  best_streak        int
  last_engagement    ISO8601 timestamp
  last_result        string (HIT/MISS)

# Commands:
HINCRBY stats:engagements:{drone_id} total_engagements 1
HINCRBY stats:engagements:{drone_id} successful_hits 1
HSET stats:engagements:{drone_id} last_result HIT last_engagement {ts}
```

### 6. Convoy Summary Cache (HASH)
**Purpose**: Aggregated convoy-level stats

```redis
KEY:    convoy:summary:{convoy_id}
TYPE:   Hash
TTL:    120 seconds (2 minutes)

FIELDS:
  convoy_callsign     string
  mission_type        string
  status              string
  drone_count         int
  airborne_count      int
  total_engagements   int
  total_hits          int
  avg_accuracy_pct    float
  avg_fuel_pct        float
  mission_start       ISO8601
  updated_at          ISO8601

# Commands:
HGETALL convoy:summary:{convoy_id}
HMSET convoy:summary:{convoy_id} ... 
```

### 7. Active Convoys List (LIST)
**Purpose**: Quick access to currently active convoy IDs

```redis
KEY:    convoys:active
TYPE:   List (or Set for uniqueness)
TTL:    300 seconds

# Commands:
RPUSH convoys:active {convoy_id}
LRANGE convoys:active 0 -1
SREM convoys:active {convoy_id}  # If using SET
```

### 8. Mesh Topology Cache (HASH)
**Purpose**: Drone mesh network adjacency

```redis
KEY:    mesh:topology:{convoy_id}
TYPE:   Hash
TTL:    30 seconds

FIELDS:
  {drone_id}: JSON array of neighbor drone_ids
  
# Example:
  "660e8400...001": '["660e8400...002", "660e8400...003"]'
  "660e8400...002": '["660e8400...001", "660e8400...004"]'

# Commands:
HGET mesh:topology:{convoy_id} {drone_id}
HSET mesh:topology:{convoy_id} {drone_id} '{neighbors_json}'
```

### 9. Waypoint Progress Cache (HASH)
**Purpose**: Current waypoint progress per drone

```redis
KEY:    waypoints:progress:{drone_id}
TYPE:   Hash
TTL:    120 seconds

FIELDS:
  total_waypoints     int (25)
  current_waypoint    int
  completed_waypoints int
  progress_pct        float
  eta_final           ISO8601
  
# Commands:
HGETALL waypoints:progress:{drone_id}
HINCRBY waypoints:progress:{drone_id} completed_waypoints 1
```

---

## TTL Strategy Summary

| Data Type | TTL | Rationale |
|-----------|-----|-----------|
| Latest Telemetry | 10s | Very hot, updated every second |
| Drone State | 60s | Moderate update frequency |
| Convoy Summary | 120s | Aggregated, less volatile |
| Leaderboard | 300s | Updated on engagement events |
| Engagement Stats | 300s | Updated on engagement events |
| Active Convoys | 300s | Moderate change frequency |
| Waypoint Progress | 120s | Updates per waypoint arrival |
| Mesh Topology | 30s | Dynamic, changes with drone positions |
| Convoy Roster | 3600s | Rarely changes during mission |

---

## Cache Invalidation Patterns

### Event-Driven Invalidation

```rust
// On new telemetry write
invalidate_keys(&[
    format!("telemetry:latest:{}", drone_id),
    format!("drone:state:{}", drone_id),
]);

// On engagement event
invalidate_keys(&[
    format!("stats:engagements:{}", drone_id),
    format!("convoy:leaderboard:{}", convoy_id),
    format!("convoy:summary:{}", convoy_id),
]);

// On drone status change
invalidate_keys(&[
    format!("drone:state:{}", drone_id),
    format!("convoy:summary:{}", convoy_id),
]);
```

### Bulk Invalidation by Convoy

```redis
# Using SCAN with pattern matching (for cleanup)
SCAN 0 MATCH convoy:*:{convoy_id}* COUNT 100
# Then DELETE matched keys
```

---

## Redis Configuration Recommendations

```redis
# Memory policy - evict least recently used on memory pressure
maxmemory-policy allkeys-lru

# Disable persistence for pure cache use case
save ""
appendonly no

# Connection pooling
tcp-keepalive 300

# Recommended memory: 2-4GB per convoy cluster
maxmemory 4gb
```

---

## Lua Scripts for Atomic Operations

### Update Leaderboard Atomically

```lua
-- KEYS[1] = convoy:leaderboard:{convoy_id}
-- KEYS[2] = stats:engagements:{drone_id}
-- ARGV[1] = drone_id
-- ARGV[2] = hit (1 or 0)

local total = redis.call('HINCRBY', KEYS[2], 'total_engagements', 1)
local hits = redis.call('HINCRBY', KEYS[2], 'successful_hits', ARGV[2])
local accuracy = (hits / total) * 100

redis.call('ZADD', KEYS[1], accuracy, ARGV[1])
redis.call('HSET', KEYS[2], 'last_result', ARGV[2] == '1' and 'HIT' or 'MISS')

return {total, hits, accuracy}
```

---

## Monitoring Keys

```redis
# Cache hit/miss tracking (application-side)
INCR cache:hits:{entity_type}
INCR cache:misses:{entity_type}

# Get hit rate
GET cache:hits:leaderboard
GET cache:misses:leaderboard
```
