import csv
import json
from urllib.request import urlopen
import zipfile
import io
from typing import Iterator, Dict, Any


def _get_countries():
    """Returns the list of all countries. Used to filter out codes that are not countries"""
    countries = urlopen(
        "https://raw.githubusercontent.com/stefangabos/world_countries/master/data/countries/_combined/countries.json"
    )
    return json.loads(countries.read())


DATASETS = {
    "emissions": "EN.ATM.CO2E.KT",
    "labor_force": "SL.TLF.TOTL.IN",
    "employment": "SL.EMP.TOTL.SP.ZS",
}


def _get_dataset(indicator: str) -> Iterator[Dict[str, Any]]:
    """Retuns the dataset from worldbank for a given indicator"""
    url = f"https://api.worldbank.org/v2/en/indicator/{indicator}?downloadformat=csv"
    data = urlopen(url).read()
    with zipfile.ZipFile(io.BytesIO(data)) as f:
        file_name = next((f for f in f.namelist() if f.startswith(f"API_{indicator}")))
        data = f.read(file_name)

    to_skip = (
        len(
            """\
    "Data Source","World Development Indicators",

    "Last Updated Date","2023-05-10",


    """
        )
        + 3
    )

    data = data.decode("utf-8-sig")[to_skip:]
    return csv.DictReader(io.StringIO(data))


def _join_dataset(indicator, values, world):
    for row in _get_dataset(DATASETS[indicator]):
        entries = [(year, row.get(str(year))) for year in range(2010, 2023)]

        country_code = row["Country Code"].lower()
        if country_code == "wld":
            world[indicator] = entries
        elif country_code in values:
            values[country_code][indicator] = entries
        else:
            continue

    # all countries are populated (inner join = left join)
    assert all(len(values[c][indicator]) != 0 for c in values)

    # find latest value
    def _get_latest(data):
        return next(filter(lambda x: x[1] != "", reversed(data)), None)

    for country_code, data in list(values.items()):
        values[country_code][indicator] = _get_latest(data[indicator])
    world[indicator] = _get_latest(world[indicator])


def _coalesce_with_world(values, world_values):
    if any(x is None for x in values.values()):
        result = world_values.copy()
        result["name"] = values["name"]
        result["origin"] = "world"
    else:
        result = values.copy()
        result["origin"] = "country"
    return result


def _process_country(data, world):
    data = _coalesce_with_world(data, world)

    # tehnically not true as we may mix the two years, but close enough
    labor_force = data["labor_force"][1]
    employees_year = data["employment"][0]
    employed_percentage = data["employment"][1]
    employed_population = int(int(labor_force) * float(employed_percentage) / 100)

    emissions_year = data["emissions"][0]
    # 10**6: data is in kilo tons of CO2 and we convert to kg CO2
    emissions = int(float(data["emissions"][1]) * 10**6)

    return {
        "name": data["name"],
        "origin": data["origin"],
        "emissions_year": emissions_year,
        "emissions_unit": "kg CO2e",
        "emissions": emissions,
        "employees_year": employees_year,
        "employees": employed_population,
        "employees_unit": "employees",
    }


def _process():
    values = {
        e["alpha3"]: {
            "name": e["en"],
            "emissions": [],
            "labor_force": [],
            "active_population": [],
        }
        for e in _get_countries()
    }

    world = {
        "emissions": [],
        "labor_force": [],
        "employment": [],
    }
    for key in DATASETS:
        _join_dataset(key, values, world)

    return [
        {"id": country_code, **_process_country(data, world)}
        for country_code, data in values.items()
    ]


entries = _process()

with open("countries.json", "w") as f:
    json.dump(entries, f, indent=4)
