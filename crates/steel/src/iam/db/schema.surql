-- SPDX-License-Identifier: AGPL-3.0-only
-- Copyright (C) 2024 Jonathan Lee
-- This program is free software: you can redistribute it and/or modify
-- it under the terms of the GNU Affero General Public License version 3
-- as published by the Free Software Foundation.
-- This program is distributed in the hope that it will be useful,
-- but WITHOUT ANY WARRANTY; without even the implied warranty of
-- MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
-- See the GNU Affero General Public License for more details.
-- You should have received a copy of the GNU Affero General Public License
-- along with this program. If not, see https://www.gnu.org/licenses/.

DEFINE TABLE user SCHEMAFULL
	PERMISSIONS
		FOR select, update, delete WHERE id = $auth.id;

DEFINE FIELD name ON user TYPE string;
DEFINE FIELD email ON user TYPE string ASSERT string::is::email($value);
DEFINE FIELD password ON user TYPE string;
DEFINE FIELD solana_did ON user TYPE option<string>;

DEFINE INDEX email ON user FIELDS email UNIQUE;

-- Role management tables
DEFINE TABLE user_roles SCHEMAFULL
	PERMISSIONS
		FOR select, create, update, delete WHERE true;

DEFINE FIELD user_email ON user_roles TYPE string ASSERT string::is::email($value);
DEFINE FIELD roles ON user_roles TYPE array<string>;
DEFINE FIELD created_at ON user_roles TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON user_roles TYPE datetime DEFAULT time::now();

DEFINE INDEX user_roles_email ON user_roles FIELDS user_email UNIQUE;

DEFINE TABLE role_definitions SCHEMAFULL
	PERMISSIONS
		FOR select WHERE true
		FOR create, update, delete WHERE $auth.roles CONTAINS 'admin';

DEFINE FIELD role_name ON role_definitions TYPE string;
DEFINE FIELD permissions ON role_definitions TYPE array<string>;
DEFINE FIELD description ON role_definitions TYPE option<string>;
DEFINE FIELD created_at ON role_definitions TYPE datetime DEFAULT time::now();

DEFINE INDEX role_definitions_name ON role_definitions FIELDS role_name UNIQUE;

-- Predefined roles
CREATE role_definitions SET
	role_name = 'admin',
	permissions = ['user:create', 'user:read', 'user:update', 'user:delete', 'role:create', 'role:read', 'role:update', 'role:delete', 'data:publish:*', 'data:consume:*'],
	description = 'System administrator with full permissions';

CREATE role_definitions SET
	role_name = 'StoreManager',
	permissions = ['data:publish:store_data_exchange', 'data:consume:store_data_exchange'],
	description = 'Store manager can publish sales and inventory data';

CREATE role_definitions SET
	role_name = 'LogisticsCoordinator',
	permissions = ['data:publish:logistics_api', 'data:consume:logistics_api'],
	description = 'Logistics coordinator can manage inventory commands';

CREATE role_definitions SET
	role_name = 'RegionalAnalyst',
	permissions = ['data:publish:analytics_service', 'data:consume:analytics_service'],
	description = 'Regional analyst can submit analytics queries';

CREATE role_definitions SET
	role_name = 'user',
	permissions = ['user:read:own'],
	description = 'Default user role with basic permissions';

DEFINE ACCESS user ON DATABASE TYPE RECORD
	SIGNUP (
		CREATE user CONTENT {
			name: $name,
			email: $email,
			password: crypto::argon2::generate($password)
		}
	)
	SIGNIN (
		SELECT * FROM user WHERE email = $email AND crypto::argon2::compare(password, $password)
	);
