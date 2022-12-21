import { Config } from "./config";
import { AVAILABLE_OPTIONS } from "./ruff_options";

function OptionEntry({
  config,
  defaultConfig,
  groupName,
  fieldName,
  onOptionChange,
}: {
  config: Config | null;
  defaultConfig: Config;
  groupName: string;
  fieldName: string;
  onOptionChange: (groupName: string, fieldName: string, value: string) => void;
}) {
  const value =
    config && config[groupName] && config[groupName][fieldName]
      ? config[groupName][fieldName]
      : "";

  return (
    <span>
      <label>
        {fieldName}
        <input
          value={value}
          placeholder={defaultConfig[groupName][fieldName]}
          type="text"
          onChange={(event) => {
            onOptionChange(groupName, fieldName, event.target.value);
          }}
        />
      </label>
    </span>
  );
}

export function Options({
  config,
  defaultConfig,
  onOptionChange,
}: {
  config: Config | null;
  defaultConfig: Config;
  onOptionChange: (groupName: string, fieldName: string, value: string) => void;
}) {
  return (
    <div className="options">
      {AVAILABLE_OPTIONS.map((group) => (
        <details key={group.name}>
          <summary>{group.name}</summary>
          <div>
            <ul>
              {group.fields.map((field) => (
                <li key={field.name}>
                  <OptionEntry
                    config={config}
                    defaultConfig={defaultConfig}
                    groupName={group.name}
                    fieldName={field.name}
                    onOptionChange={onOptionChange}
                  />
                </li>
              ))}
            </ul>
          </div>
        </details>
      ))}
    </div>
  );
}
