# PacMine Daemon API Documents

**Version:** 1.0  
**Protocol:** HTTP over TCP or Unix Socket

**Base URL**

The API can be accessed using either TCP or Unix Socket(*nix only):

| Transport   | Example                          | Notes                                                                 |
|:------------|:---------------------------------|:----------------------------------------------------------------------|
| TCP         | `http://127.0.0.1:8080`          | Access via TCP socket.                                                |
| Unix Socket | `unix://$HOME/.pacmine/api.sock` | Use `curl --unix-socket $HOME/.pacmine/api.sock http://localhost/...` |

**Authentication:**  
All endpoints (except `/control/status` and `/ws/{terminal token}`) require authentication via the following header:
`Authorization: Bearer {Your API Token}`

## Control

### Status

Get the current running status of the daemon.

* Endpoint

| Method | Path              |
|:-------|:------------------|
| GET    | `/control/status` |

* Example

```
curl -X GET http://localhost/control/status
```

* Response

```
{
  "success": true
  "version": 1.0
}
```

|    Key    |   Type   | Description                                     |
|:---------:|:--------:|:------------------------------------------------|
| `success` |  `bool`  | Indicates whether the operation was successful. |
| `version` | `number` | API version                                     |

### List

Get a list of projects, containing simple information.

* Endpoint

| Method | Path            |
|:-------|:----------------|
| GET    | `/control/list` |

* Request

Headers:

```
Authorization: Bearer {Your API Token}
```

* Example

```
curl -X GET http://localhost/control/list \
     -H "Authorization: Bearer {Your API Token}"
```

* Response(success)

```
{
  "success": true,
  "projects": [
    {
      "id": 1,
      "running": true,
      "name": "MyServer",
      "server_type": "Vanilla",
      "version": "1.21.10"
    },
    {
      "id": 2,
      "running": false,
      "name": "MyServer",
      "server_type": "Vanilla",
      "version": "1.21.10"
    }
  ]
}
```

|    Key     |       Type        | Description                                     |
|:----------:|:-----------------:|:------------------------------------------------|
| `success`  |      `bool`       | Indicates whether the operation was successful. |
| `projects` | `array`(`object`) | The list of projects.                           |

Project Object:

|      Key      |   Type   | Description                     |
|:-------------:|:--------:|:--------------------------------|
|     `id`      | `number` | Project ID                      |
|   `running`   |  `bool`  | Whether the project is running. |
|    `name`     | `string` | Name of the project.            |
| `server_type` | `string` | Type of the server.             |
|   `version`   | `string` | Version of the server.          |

### Add

Add an existing project from the server.

* Endpoint

| Method | Path           |
|:-------|:---------------|
| POST   | `/control/add` |

* Request

Headers:

```
Content-Type: application/json
Authorization: Bearer {Your API Token}
```

Body:

```
{
  "path": "/path/to/project/dir"
}
```

* Example

```
curl -X POST http://localhost/control/add \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer {Your API Token}" \
    -d '{"path": "/path/to/project/dir"}'
```

* Response(success)

```
{
  "success": true
}
```

### Create

Create a server.

* Endpoint

| Method | Path              |
|:-------|:------------------|
| POST   | `/control/create` |

* Request

Headers:

```
Content-Type: application/toml
Authorization: Bearer {Your API Token}
```

Body:

You need to upload the full PacMine project configuration file as the request body.

```
[project]
name = "MyServer"
server_type = "Paper"
version = "1.21.10"
version_type = "release"
execute = "server.jar"
birthday = "2025-11-01T04:29:28.685796400Z"

[runtime.java]
mode = "auto"
edition = "OpenJDK"
version = 21
custom = ""
arguments = []
xms = 0
xmx = 0

[backup]
enable = true
world = true
other = false

[backup.time]
interval = 0
cron = ""

[backup.event]
start = false
stop = true
update = true

[plugin_manage]
manage = true
```

* Example

```
curl -X POST http://localhost/control/create \
    -H "Content-Type: application/toml" \
    -H "Authorization: Bearer {Your API Token}" \
    -d '
[project]
name = "MyServer"
server_type = "Paper"
version = "1.21.10"
version_type = "release"
execute = "server.jar"
birthday = "2025-11-01T04:29:28.685796400Z"

[runtime.java]
mode = "auto"
edition = "OpenJDK"
version = 21
custom = ""
arguments = []
xms = 0
xmx = 0

[backup]
enable = true
world = true
other = false

[backup.time]
interval = 0
cron = ""

[backup.event]
start = false
stop = true
update = true

[plugin_manage]
manage = true
'
```

* Response(success)

```
{
  "success": true
}
```

### Remove

Remove a project by its ID. **Only applicable to projects created via the "Create" API.**

* Endpoint

| Method | Path                           |
|:-------|:-------------------------------|
| GET    | `/control/remove/{project id}` |

* Request

Headers:

```
Authorization: Bearer {Your API Token}
```

* Example

```
curl -X GET http://localhost/control/remove/{project id} \
    -H "Authorization: Bearer {Your API Token}"
```

* Response(success)

```
{
  "success": true
}
```

## Project

### Start

Start a server.

* Endpoint

| Method | Path                          |
|:-------|:------------------------------|
| GET    | `/project/{project id}/start` |

* Request

Headers:

```
Authorization: Bearer {Your API Token}
```

* Example

```
curl -X GET http://localhost/project/{project id}/start \
    -H "Authorization: Bearer {Your API Token}"
```

* Response(success)

```
{
  "success": true
}
```

### Stop

Stop a server.

* Endpoint

| Method | Path                         |
|:-------|:-----------------------------|
| GET    | `/project/{project id}/stop` |

* Request

Headers:

```
Authorization: Bearer {Your API Token}
```

* Example

```
curl -X GET http://localhost/project/{project id}/stop \
    -H "Authorization: Bearer {Your API Token}"
```

* Response(success)

```
{
  "success": true
}
```

### Download

Download a file.

Used to modify configuration files

* Endpoint

| Method | Path                             |
|:-------|:---------------------------------|
| POST   | `/project/{project id}/download` |

* Request

Headers:

```
Content-Type: application/json
Authorization: Bearer {Your API Token}
```

Body:

```
{
  "path": "/path/to/file"
}
```

* Example

```
curl -X POST http://localhost/project/{project id}/download \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer {Your API Token}" \
    -d '{"path": "/path/to/file"}'
```

* Response(success)

```
{
  "success": true,
  "file": "contents of the file"
}
```

### Upload

Modify a file in the project.

Used to modify configuration files.

* Endpoint

| Method | Path                           |
|:-------|:-------------------------------|
| POST   | `/project/{project id}/upload` |

* Request

Headers:

```
Content-Type: multipart/form-data
Authorization: Bearer {Your API Token}
```

Form Data:

`path` is the remote path, the root directory is the project directory, and `file` points to the local file.

```
path=server.properties
file=@server.properties
```

* Example

```
curl -X POST http://localhost/project/{project id}/upload \
    -H "Content-Type: multipart/form-data" \
    -H "Authorization: Bearer {Your API Token}" \
    -F "path=server.properties" \
    -F "file=@server.properties"
```

* Response(success)

```
{
  "success": true
}
```

### Connect

Generate a streaming interface to connect to the terminal.

* Endpoint

| Method | Path                            |
|:-------|:--------------------------------|
| GET    | `/project/{project id}/connect` |

* Request

Headers:

```
Authorization: Bearer {Your API Token}
```

* Example

```
curl -X GET http://localhost/project/{project id}/connect \
    -H "Authorization: Bearer {Your API Token}"
```

* Response(success)

```
{
  "success": true,
  "path": "/ws/7a09d5cb-01ac-41ff-bf10-9e29d98efe14"
}
```

|    Key    |   Type   | Description                         |
|:---------:|:--------:|:------------------------------------|
| `success` |  `bool`  | Whether the request was successful. |
|  `path`   | `string` | Interface address.                  |

> You need to connect to this [interface](#websocket) using the WebSocket protocol. If there is no connection for some
> time, the interface will be closed.

## WebSocket

* **Protocol**: WebSocket over HTTP

Connect to the terminal at the path given by `/project/{project id}/connect`

The client must start sending and receiving frames after a successful 101 response.

* Endpoint

| Method | Path                   |
|:-------|:-----------------------|
| GET    | `/ws/{terminal token}` |

* Request

Headers:

```
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: {Sec-WebSocket-Key}
Sec-WebSocket-Version: 13
```

* Example(Test only)

```
curl -i -N \
    -X GET http://localhost/ws/{terminal token} \
    -H "Upgrade: websocket" \
    -H "Connection: Upgrade" \
    -H "Sec-WebSocket-Key: {Sec-WebSocket-Key}" \
    -H "Sec-WebSocket-Version: 13"
```

After a successful handshake, the server responds with:

```
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: {computed value}
```

> The Sec-WebSocket-Key must be a random base64-encoded 16-byte value. The server computes Sec-WebSocket-Accept =
> base64(SHA1(key + UUID)).

## Appendix

### Error Response

```
{
  "success": false,
  "error": "Invalid token"
}
```

|    Key    |   Type   | Description                          |
|:---------:|:--------:|:-------------------------------------|
| `success` |  `bool`  | Always `false` when an error occurs. |
|  `error`  | `string` | Human-readable error message.        |