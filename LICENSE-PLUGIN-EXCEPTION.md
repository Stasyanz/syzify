# Syzify Plugin Exception, version 1.0

*Additional permission under section 7 of the GNU Affero General Public
License, version 3 ("AGPLv3").*

Copyright (C) 2026 Stanislav Zainullin

## Preamble (non-binding)

Syzify loads plugins into its own process and exposes a documented Plugin
API to them. Under a strict reading of the AGPLv3, a plugin combined with
Syzify in this way could be considered part of a single covered work, which
would require every plugin to be licensed under the AGPLv3 as well. The
copyright holders of Syzify do not want that outcome: plugin authors —
including commercial authors — should be free to license their plugins as
they see fit, as long as they interact with Syzify only through the official
Plugin API. This exception grants that freedom. It is an *additional
permission* as provided for by section 7 of the AGPLv3: it grants extra
rights on top of the AGPLv3 and takes nothing away from it. The full text
of the AGPLv3 is in the `LICENSE` file at the root of this repository.

## Definitions

- **"The Program"** means Syzify, the covered work licensed under the
  AGPLv3 with this exception.
- **"Plugin API"** means the interfaces that the Program officially provides
  to plugins, as documented in the source code and documentation of the
  version of the Program with which an Independent Module is combined or
  designed to be combined: the plugin manifest format (`plugin.json`), the
  host functions and data structures exposed to plugin code by the Program's
  plugin runtime (the "Host SDK"), the ViewSpec rendering schema, and the
  `.syzify-ext` package format. An Independent Module's target version may
  be indicated, for example, by the compatibility declaration in its plugin
  manifest. Changes to that documentation in later versions of the Program
  do not retroactively narrow the Plugin API with respect to Independent
  Modules designed for an earlier version.
- **"Interface Material"** means the interface definitions, type
  declarations, manifest schemas, and official SDK libraries (such as
  bindings to the Host SDK) that the Program's authors provide, and identify
  as such in the source code and documentation of the version of the Program
  described in the definition of "Plugin API" above, for the purpose of
  developing plugins.
- **"Independent Module"** means a plugin, extension, or other software
  module that (a) interacts with the Program exclusively through the Plugin
  API, and (b) is not itself derived from the source code of the Program,
  except for the use, inclusion, or adaptation of Interface Material.

## Grant of Additional Permission

As a special exception to the terms of the AGPLv3, the copyright holders of
the Program give you permission to:

1. develop, run, copy, convey, and monetize Independent Modules under
   license terms of your own choosing, without any obligation arising from
   the Program's license as to the licensing of the Independent Module
   itself;
2. combine or link Independent Modules with the Program, including loading
   them into the Program's process and executing them within the Program's
   plugin runtime, without causing the Independent Module to become subject
   to the terms of the AGPLv3;
3. convey the resulting combination of the Program and Independent Modules,
   provided that the terms of the AGPLv3 (together with this exception) are
   fulfilled for the Program itself; and
4. include Interface Material, in unmodified or modified form, in an
   Independent Module and convey that Interface Material, as part of the
   Independent Module, under the license terms of the Independent Module,
   provided that copyright and license notices contained in the Interface
   Material are preserved.

An Independent Module that ceases to satisfy the conditions of the
definition above — for example, by incorporating source code of the Program
beyond the Interface Material, or by interacting with the
Program other than through the Plugin API — is not covered by this
exception, and the ordinary terms of the AGPLv3 apply to the extent they
otherwise would.

## Notes

- As provided by section 7 of the AGPLv3, you may remove this additional
  permission from copies you convey. If you convey the Program (modified or
  unmodified) without this exception, plugins for your version do not enjoy
  the permissions granted here.
- If you modify the Program, you may choose whether to extend this
  exception to your modified version; you are not required to do so.
- This exception applies to the Program as a whole, including every source
  file in this repository, unless a file states otherwise.