## ADDED Requirements

### Requirement: Recording Persistence
The system SHALL persist recordings in a Room database with fields: id (UUID), name, startedAt, endedAt, carrier, and notes.

#### Scenario: Create a new recording
- **WHEN** the user starts a recording
- **THEN** a new Recording row is inserted with a generated UUID, auto-name ("Trip YYYY-MM-DD HH:mm"), current timestamp as startedAt, and null endedAt

#### Scenario: Stop a recording
- **WHEN** the user stops a recording
- **THEN** the Recording row is updated with endedAt set to the current timestamp

#### Scenario: Delete a recording
- **WHEN** the user deletes a recording
- **THEN** the Recording row and all associated Measurement rows are deleted (CASCADE)

#### Scenario: Discard empty recording
- **WHEN** a recording is stopped with 0 measurements
- **THEN** the recording is automatically deleted rather than saved

### Requirement: Measurement Persistence
The system SHALL persist measurements in a Room database with fields: id (auto-increment), recordingId (FK), timestamp, lat, lng, accuracyM, speedKmh, bearing, altitude, networkType, dataState, and cellsJson (JSON-serialized list of CellMeasurement).

#### Scenario: Batch insert measurements
- **WHEN** the recording service flushes its buffer (20 measurements or 30 seconds)
- **THEN** all buffered measurements are inserted in a single transaction

#### Scenario: Query measurements for a recording
- **WHEN** the UI requests measurements for a recording
- **THEN** measurements are returned ordered by timestamp ascending as a Flow

### Requirement: Domain Models
The system SHALL define domain models: Recording, Measurement, CellMeasurement, GeoPoint, CellTech (GSM/WCDMA/LTE/NR/CDMA/UNKNOWN), NetworkType, DataState, and SignalLevel (NONE/POOR/FAIR/GOOD/EXCELLENT).

Signal level is computed from LTE RSRP using these thresholds: EXCELLENT >= -85 dBm, GOOD -86 to -95 dBm, FAIR -96 to -105 dBm, POOR -106 to -115 dBm, NONE < -115 dBm. NR uses similar thresholds; WCDMA uses RSCP; GSM uses RSSI.

#### Scenario: Signal level computation from LTE RSRP
- **WHEN** an LTE RSRP value of -90 dBm is measured
- **THEN** the computed SignalLevel is GOOD
